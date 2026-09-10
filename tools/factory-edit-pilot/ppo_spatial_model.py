"""Strict specifications for spatial reach, actor/critic factors and shared capacity."""

SCHEMA = 'ppo-actor-spatial-reach-v1'
SHARED_CAPACITY_SCHEMA = 'ppo-shared-capacity-v1'
VARIANTS = {'cnn2': [1, 1], 'cnn4': [1, 1, 1, 1],
            'cnn4-dilated': [1, 1, 2, 4], 'cnn8': [1] * 8}


def specification(name):
    return dict(schema=SCHEMA, architecture='mean-message-residual-cnn', graph_layers=2,
                width=32, cnn_layers=len(VARIANTS[name]), cnn_dilations=list(VARIANTS[name]),
                spatial_blocks='conv-silu-identity-residual-no-extra-normalization',
                critic_graph_layers=2, critic_width=32, critic_cnn_dilations=[1, 1],
                critic_seed_offset=100000)


def shared_capacity_specification(critic_history=True, critic_after_kl=False):
    from ppo_actor_critic import treatment
    return dict(specification('cnn2'), schema=SHARED_CAPACITY_SCHEMA,
                width=40, critic_width=40,
                actor_critic=treatment(True, critic_history, critic_after_kl))


def validate_model(model):
    if model.get('schema') == SHARED_CAPACITY_SCHEMA:
        from ppo_actor_critic import validate
        config = model.get('actor_critic', {})
        validate(config)
        if model != shared_capacity_specification(config['critic_history'], config['critic_after_kl']):
            raise ValueError('ppo-spatial.model-spec: shared capacity requires the fixed width-40 shared cnn2 backbone')
        return
    if 'actor_critic' in model:
        from ppo_actor_critic import validate
        validate(model['actor_critic'])
        base = {k: v for k, v in model.items() if k != 'actor_critic'}
        if base != specification('cnn2'):
            raise ValueError('ppo-spatial.model-spec: actor/critic combinations require the fixed cnn2 backbone')
        return
    # This bounded screen intentionally admits only the four predeclared treatments.
    if not any(model == specification(name) for name in VARIANTS):
        raise ValueError('ppo-spatial.model-spec: expected one of the four versioned spatial-reach specifications')
