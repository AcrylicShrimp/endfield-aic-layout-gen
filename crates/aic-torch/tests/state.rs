#![cfg(feature = "libtorch")]
use aic_torch::{
    Adam, AdamBackend, AdamState, Generator,
    tch::{Device, Kind, Tensor},
};

fn optimizer(p: &Tensor) -> Adam {
    Adam::new(
        vec![("weight".into(), p.shallow_clone())],
        0.01,
        0.9,
        0.999,
        1e-8,
        AdamBackend::Foreach,
    )
    .unwrap()
}
#[test]
fn independent_generators_resume_all_draw_kinds() {
    let mut a = Generator::new(71).unwrap();
    let mut other = Generator::new(29).unwrap();
    let _ = a.permutation(7).unwrap();
    let state = a.state().unwrap();
    let _ = other.integers(100, 19).unwrap();
    let mut restored = Generator::new(0).unwrap();
    restored.restore(&state).unwrap();
    assert!(
        a.permutation(23)
            .unwrap()
            .equal(&restored.permutation(23).unwrap())
    );
    assert!(
        a.integers(17, 43)
            .unwrap()
            .equal(&restored.integers(17, 43).unwrap())
    );
    let p = Tensor::from_slice(&[0.1f32, 0.2, 0.7]);
    assert!(
        a.categorical(&p)
            .unwrap()
            .equal(&restored.categorical(&p).unwrap())
    );
    assert!(a.state().unwrap().equal(&restored.state().unwrap()));
    assert!(
        a.restore(&Tensor::zeros([8], (Kind::Uint8, Device::Cpu)))
            .is_err()
    );
    assert!(
        a.restore(&Tensor::zeros([8], (Kind::Float, Device::Cpu)))
            .is_err()
    );
}
#[test]
fn nonempty_adam_restore_continues_exactly_and_missing_grad_does_not_advance() {
    let p = Tensor::from_slice(&[0.4f32, -0.2]).set_requires_grad(true);
    let mut a = optimizer(&p);
    assert!(a.snapshot().unwrap()[0].1.is_none());
    p.square().sum(Kind::Float).backward();
    a.clip_grad_norm(0.5).unwrap();
    a.step().unwrap();
    let state = a.snapshot().unwrap().remove(0).1.unwrap();
    assert_eq!(state.step, 1);
    a.zero_grad().unwrap();
    a.step().unwrap();
    assert_eq!(a.snapshot().unwrap()[0].1.as_ref().unwrap().step, 1);
    let q = p.detach().copy().set_requires_grad(true);
    let mut b = optimizer(&q);
    b.restore_parameter(0, &state).unwrap();
    for (parameter, adam) in [(&p, &mut a), (&q, &mut b)] {
        parameter.square().sum(Kind::Float).backward();
        adam.clip_grad_norm(0.5).unwrap();
        adam.step().unwrap();
    }
    assert!(p.equal(&q));
    let x = a.snapshot().unwrap().remove(0).1.unwrap();
    let y = b.snapshot().unwrap().remove(0).1.unwrap();
    assert_eq!(x.step, y.step);
    assert!(x.mean.equal(&y.mean) && x.square.equal(&y.square));
    // Detached snapshots cannot change optimizer-owned state.
    let _ = state.mean.shallow_clone().fill_(100.);
    assert!(
        a.snapshot().unwrap()[0]
            .1
            .as_ref()
            .unwrap()
            .mean
            .equal(&x.mean)
    );
}
#[test]
fn malformed_optimizer_state_and_unsupported_mode_fail() {
    let p = Tensor::ones([2], (Kind::Float, Device::Cpu)).set_requires_grad(true);
    let mut a = optimizer(&p);
    for state in [
        AdamState {
            step: i64::MAX,
            mean: p.detach(),
            square: p.detach(),
        },
        AdamState {
            step: -1,
            mean: p.detach(),
            square: p.detach(),
        },
        AdamState {
            step: 1,
            mean: p.detach(),
            square: -p.detach(),
        },
        AdamState {
            step: 1,
            mean: Tensor::zeros([3], (Kind::Float, Device::Cpu)),
            square: p.detach(),
        },
        AdamState {
            step: 16_777_217,
            mean: p.detach(),
            square: p.detach(),
        },
    ] {
        assert!(a.restore_parameter(0, &state).is_err());
    }
    assert!(a.snapshot().unwrap()[0].1.is_none());
    assert!(
        Adam::new(
            vec![("p".into(), p)],
            0.01,
            0.9,
            0.999,
            1e-8,
            AdamBackend::Fused
        )
        .is_err()
    );
}
