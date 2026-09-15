//! Missing references must preserve both node and context gradients exactly.
use super::*;
use aic_torch::tch::Kind;

fn masked_reference_parity(device: Device) {
    aic_torch::deterministic().unwrap();
    for node_count in [1, 7, 137] {
        for missing in [0, 1, 3, 8] {
            let candidates = 1024;
            let refs: Vec<i64> = (0..candidates * 2)
                .map(|i| {
                    if i % 8 < missing {
                        -1
                    } else {
                        (i / 5) % node_count
                    }
                })
                .collect();
            let refs = Tensor::from_slice(&refs)
                .view([candidates, 2])
                .to_device(device);
            let nodes = (Tensor::arange(node_count * 40, (Kind::Float, device)) * 0.17)
                .sin()
                .view([node_count, 40]);
            let context = (Tensor::arange(candidates * 40, (Kind::Float, device)) * 0.13)
                .cos()
                .view([candidates, 40]);
            let upstream = (Tensor::arange(candidates * 80, (Kind::Float, device)) * 0.11)
                .sin()
                .view([candidates, 2, 40]);
            let run = |spread: bool| {
                let nodes = nodes.copy().set_requires_grad(true);
                let context = context.copy().set_requires_grad(true);
                let selected = if spread {
                    masked_references(&nodes, &refs, &context)
                } else {
                    nodes
                        .index(&[Some(&refs.clamp_min(0))])
                        .where_self(&refs.ge(0).unsqueeze(-1), &context.unsqueeze(1))
                };
                (&selected * &upstream).sum(Kind::Float).backward();
                (selected.detach(), nodes.grad(), context.grad())
            };
            let expected = run(false);
            for _ in 0..2 {
                let actual = run(true);
                assert!(
                    expected.0.equal(&actual.0),
                    "selected: nodes={node_count} missing={missing}"
                );
                assert!(
                    expected.1.equal(&actual.1),
                    "node gradient: nodes={node_count} missing={missing}"
                );
                assert!(
                    expected.2.equal(&actual.2),
                    "context gradient: nodes={node_count} missing={missing}"
                );
            }
        }
    }
}

#[test]
fn masked_references_preserve_cpu_forward_and_backward() {
    masked_reference_parity(Device::Cpu);
}

#[test]
fn masked_references_preserve_cuda_forward_and_backward() {
    if aic_torch::tch::Cuda::is_available() {
        masked_reference_parity(Device::Cuda(0));
    }
}
