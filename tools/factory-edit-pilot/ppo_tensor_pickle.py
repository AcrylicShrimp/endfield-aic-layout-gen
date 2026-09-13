"""Serialize immutable tensor values directly, without copying their object tree."""
import io
import pickle

import torch


class Pickler(pickle.Pickler):
    def reducer_override(self, value):
        if type(value) is torch.Tensor and value.layout == torch.strided and not value.requires_grad:
            try:
                array = value.detach().cpu().numpy()
            except (TypeError, RuntimeError):
                # Unsupported NumPy dtypes retain Torch's native representation.
                # Empty shared views must own their zero-byte storage (the
                # historical mixed-dtype/nonzero-address serialization failure).
                if value.numel() == 0:
                    return value.clone().__reduce_ex__(5)
            else:
                return torch.from_numpy, (array,)
        return NotImplemented


def dumps(value, *, buffer_callback=None):
    stream = io.BytesIO()
    Pickler(stream, protocol=5, buffer_callback=buffer_callback).dump(value)
    return stream.getvalue()


# torch.save accepts a pickle module. Resulting artifacts remain standard Torch
# archives readable by torch.load(weights_only=False), including old archives.
dump = pickle.dump
load = pickle.load
loads = pickle.loads
Unpickler = pickle.Unpickler
