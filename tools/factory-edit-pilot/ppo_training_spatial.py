"""CPU address plans and bucket-wide GPU spatial feature gathers."""
import numpy as np
import torch


def prepare_samples(worlds, buckets, requests, device, spatial_shapes=None):
    """Requests are ordered (world index, xy array) pairs; -1 means zero features."""
    membership = {world: (bucket, slot) for bucket, ids in enumerate(buckets) for slot, world in enumerate(ids)}
    plans = [dict() for _ in buckets]
    outputs = {}
    for name, groups in requests.items():
        addresses, destinations = ([[] for _ in buckets] for _ in range(2))
        count = 0
        for world, xy in groups:
            if world >= 0 and len(xy):
                bucket, slot = membership[world]
                height, width = spatial_shapes[bucket] if spatial_shapes else worlds[world]['raster'].shape[1:]
                addresses[bucket].append(slot * height * width + xy[:, 1] * width + xy[:, 0])
                destinations[bucket].append(np.arange(count, count + len(xy), dtype=np.int64))
            count += len(xy)
        restore = np.zeros(count, dtype=np.int64)
        offset = 1
        for bucket in range(len(buckets)):
            if addresses[bucket]:
                index = np.concatenate(addresses[bucket])
                restore[np.concatenate(destinations[bucket])] = np.arange(offset, offset + len(index))
                offset += len(index)
                plans[bucket][name] = torch.as_tensor(index, dtype=torch.long, device=device)
        outputs[name] = torch.as_tensor(restore, dtype=torch.long, device=device)
    return plans, outputs


def gather_samples(raster, indices):
    # One compact channel vector per pixel; addresses do not repeat per channel.
    pixels = raster.permute(0, 2, 3, 1).reshape(-1, raster.shape[1])
    return {name: pixels.index_select(0, index) for name, index in indices.items()}


def restore_samples(parts, orders, prototype):
    zero = prototype.new_zeros(1, prototype.shape[1])
    return {name: torch.cat([zero, *parts[name]], 0).index_select(0, order)
            for name, order in orders.items()}


def padded_rasters(worlds, device):
    height = max(w['raster'].shape[1] for w in worlds)
    width = max(w['raster'].shape[2] for w in worlds)
    channels = worlds[0]['raster'].shape[0]
    raster = np.zeros((len(worlds), channels, height, width), np.float32)
    mask = np.zeros((len(worlds), 1, height, width), np.float32)
    for i, world in enumerate(worlds):
        h, w = world['raster'].shape[1:]
        raster[i, :, :h, :w] = world['raster']
        mask[i, :, :h, :w] = 1
    return torch.as_tensor(raster, device=device), torch.as_tensor(mask, device=device)


def encode_padded_raster(encoder, raster, mask):
    """Preserve per-factory GroupNorm and zero boundary at every spatial layer."""
    for layer in encoder.raster:
        if isinstance(layer, torch.nn.GroupNorm):
            batch, channels, height, width = raster.shape
            grouped = raster.reshape(batch, layer.num_groups, channels // layer.num_groups, height, width)
            valid = mask.unsqueeze(1)
            count = valid.sum((2, 3, 4), keepdim=True) * (channels // layer.num_groups)
            mean = (grouped * valid).sum((2, 3, 4), keepdim=True) / count
            centered = grouped - mean
            variance = (centered.square() * valid).sum((2, 3, 4), keepdim=True) / count
            raster = (centered * torch.rsqrt(variance + layer.eps)).reshape_as(raster)
            if layer.affine:
                raster = raster * layer.weight[None, :, None, None] + layer.bias[None, :, None, None]
        else:
            raster = layer(raster)
            if isinstance(layer, torch.nn.Conv2d):
                raster = raster.float()
        raster = raster * mask
    for convolution in encoder.spatial_blocks:
        raster = (raster + torch.nn.functional.silu(convolution(raster).float())) * mask
    return raster
