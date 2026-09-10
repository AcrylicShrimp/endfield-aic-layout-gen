"""Versioned actor spatial-reach experiment; the independent critic is fixed."""

SCHEMA = 'ppo-actor-spatial-reach-v1'
VARIANTS = {'cnn2': [1, 1], 'cnn4': [1, 1, 1, 1],
            'cnn4-dilated': [1, 1, 2, 4], 'cnn8': [1] * 8}


def specification(name):
    return dict(schema=SCHEMA, architecture='mean-message-residual-cnn', graph_layers=2,
                width=32, cnn_layers=len(VARIANTS[name]), cnn_dilations=list(VARIANTS[name]),
                spatial_blocks='conv-silu-identity-residual-no-extra-normalization',
                critic_graph_layers=2, critic_width=32, critic_cnn_dilations=[1, 1],
                critic_seed_offset=100000)


def validate_model(model):
    # This bounded screen intentionally admits only the four predeclared treatments.
    if not any(model == specification(name) for name in VARIANTS):
        raise ValueError('ppo-spatial.model-spec: expected one of the four versioned spatial-reach specifications')
