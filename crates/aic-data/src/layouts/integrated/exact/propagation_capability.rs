use pumpkin_solver::Solver;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::results::CSPSolverExecutionFlag;
use pumpkin_solver::core::variables::TransformableVariable;

fn propagate(solver: &mut Solver) {
    assert_eq!(
        solver.propagate_to_fixpoint(),
        CSPSolverExecutionFlag::Feasible
    );
}

#[test]
fn element_does_not_remove_an_index_supported_only_by_an_interior_hole() {
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let index = solver.new_named_bounded_integer(0, 2, "element-index");
    let geometry = solver.new_named_sparse_integer([4, 42, 80], "element-geometry");
    solver
        .add_constraint(pumpkin_solver::element(index, [4, 42, 80], geometry, tag))
        .post();
    propagate(&mut solver);

    solver.add_clause([geometry.disequality_predicate(42)], tag);
    propagate(&mut solver);

    assert!(
        solver.contains(&index, 1),
        "Pumpkin 0.5 Element currently compares bounds rather than interior value support"
    );
}

#[test]
fn three_term_linear_equality_does_not_copy_interior_holes() {
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let left = solver.new_named_sparse_integer([4, 42, 80], "linear-left");
    let right = solver.new_named_sparse_integer([4, 42, 80], "linear-right");
    let zero = solver.new_named_bounded_integer(0, 0, "linear-zero");
    solver
        .add_constraint(pumpkin_solver::equals(
            vec![left.scaled(1), right.scaled(-1), zero.scaled(1)],
            0,
            tag,
        ))
        .post();
    propagate(&mut solver);

    solver.add_clause([left.disequality_predicate(42)], tag);
    propagate(&mut solver);

    assert!(
        solver.contains(&right, 42),
        "three-or-more-term equality currently performs bounds propagation"
    );
}

#[test]
fn binary_equality_copies_interior_holes_bidirectionally() {
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let left = solver.new_named_sparse_integer([4, 42, 80], "binary-left");
    let right = solver.new_named_sparse_integer([4, 42, 80], "binary-right");
    solver
        .add_constraint(pumpkin_solver::binary_equals(left, right, tag))
        .post();
    propagate(&mut solver);

    solver.add_clause([left.disequality_predicate(42)], tag);
    propagate(&mut solver);

    assert!(!solver.contains(&right, 42));
}

#[test]
fn positive_table_removes_values_after_their_last_row_support_disappears() {
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let placement = solver.new_named_bounded_integer(0, 2, "table-placement");
    let port = solver.new_named_bounded_integer(0, 0, "table-port");
    let geometry = solver.new_named_sparse_integer([4, 42, 80], "table-geometry");
    solver
        .add_constraint(pumpkin_solver::table(
            vec![placement, port, geometry],
            vec![vec![0, 0, 4], vec![1, 0, 42], vec![2, 0, 80]],
            tag,
        ))
        .post();
    propagate(&mut solver);

    solver.add_clause([geometry.disequality_predicate(42)], tag);
    propagate(&mut solver);

    assert!(!solver.contains(&placement, 1));
}

#[test]
fn positive_table_does_not_eagerly_remove_values_absent_from_its_projection() {
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let placement = solver.new_named_bounded_integer(0, 1, "table-placement");
    let geometry = solver.new_named_bounded_integer(4, 80, "table-geometry");
    solver
        .add_constraint(pumpkin_solver::table(
            vec![placement, geometry],
            vec![vec![0, 4], vec![1, 80]],
            tag,
        ))
        .post();
    propagate(&mut solver);

    assert!(
        solver.contains(&geometry, 42),
        "table comparisons need the same sparse projected input domains"
    );
}
