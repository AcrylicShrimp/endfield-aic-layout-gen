"""Queue prepared tensor copies without synchronizing every individual leaf."""
import torch


def map_tensors(value, function):
    if isinstance(value, torch.Tensor):
        return function(value)
    if isinstance(value, dict):
        return {key: map_tensors(item, function) for key, item in value.items()}
    if isinstance(value, list):
        return [map_tensors(item, function) for item in value]
    if isinstance(value, tuple):
        return tuple(map_tensors(item, function) for item in value)
    return value


def move_prepared(value, device):
    device = torch.device(device)
    return map_tensors(value, lambda tensor: tensor.to(device, non_blocking=device.type == 'cuda'))
