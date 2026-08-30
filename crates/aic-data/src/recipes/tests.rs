use super::*;

fn valid_book() -> RecipeBook {
    RecipeBook {
        schema_version: 1,
        external_items: vec!["originium-ore".to_string()],
        recipes: vec![Recipe {
            id: "grind-originium-powder".to_string(),
            facility: "grinding-unit".to_string(),
            inputs: vec![ItemAmount {
                item: "originium-ore".to_string(),
                quantity: 1,
            }],
            outputs: vec![ItemAmount {
                item: "originium-powder".to_string(),
                quantity: 1,
            }],
            duration_ms: 2000,
        }],
    }
}

fn multi_step_book() -> RecipeBook {
    RecipeBook {
        schema_version: 1,
        external_items: vec!["originium-ore".to_string()],
        recipes: vec![
            Recipe {
                id: "grind-originium-powder".to_string(),
                facility: "grinding-unit".to_string(),
                inputs: vec![ItemAmount {
                    item: "originium-ore".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "originium-powder".to_string(),
                    quantity: 1,
                }],
                duration_ms: 2000,
            },
            Recipe {
                id: "smelt-originium-ingot".to_string(),
                facility: "smelter".to_string(),
                inputs: vec![ItemAmount {
                    item: "originium-powder".to_string(),
                    quantity: 2,
                }],
                outputs: vec![ItemAmount {
                    item: "originium-ingot".to_string(),
                    quantity: 1,
                }],
                duration_ms: 5000,
            },
        ],
    }
}

fn multi_output_book() -> RecipeBook {
    RecipeBook {
        schema_version: 1,
        external_items: vec!["originium-ore".to_string()],
        recipes: vec![
            Recipe {
                id: "split-originium-ore".to_string(),
                facility: "separator".to_string(),
                inputs: vec![ItemAmount {
                    item: "originium-ore".to_string(),
                    quantity: 1,
                }],
                outputs: vec![
                    ItemAmount {
                        item: "originium-powder".to_string(),
                        quantity: 1,
                    },
                    ItemAmount {
                        item: "originium-shard".to_string(),
                        quantity: 1,
                    },
                ],
                duration_ms: 2000,
            },
            Recipe {
                id: "assemble-originium-core".to_string(),
                facility: "assembler".to_string(),
                inputs: vec![
                    ItemAmount {
                        item: "originium-powder".to_string(),
                        quantity: 1,
                    },
                    ItemAmount {
                        item: "originium-shard".to_string(),
                        quantity: 1,
                    },
                ],
                outputs: vec![ItemAmount {
                    item: "originium-core".to_string(),
                    quantity: 1,
                }],
                duration_ms: 1000,
            },
        ],
    }
}

fn surplus_book() -> RecipeBook {
    RecipeBook {
        schema_version: 1,
        external_items: vec!["originium-ore".to_string()],
        recipes: vec![Recipe {
            id: "split-originium-ore".to_string(),
            facility: "separator".to_string(),
            inputs: vec![ItemAmount {
                item: "originium-ore".to_string(),
                quantity: 1,
            }],
            outputs: vec![
                ItemAmount {
                    item: "originium-powder".to_string(),
                    quantity: 2,
                },
                ItemAmount {
                    item: "originium-shard".to_string(),
                    quantity: 1,
                },
            ],
            duration_ms: 2000,
        }],
    }
}

fn same_facility_chain_book() -> RecipeBook {
    RecipeBook {
        schema_version: 1,
        external_items: vec!["raw-a".to_string(), "raw-b".to_string()],
        recipes: vec![
            Recipe {
                id: "make-a".to_string(),
                facility: "assembler".to_string(),
                inputs: vec![ItemAmount {
                    item: "raw-a".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "item-a".to_string(),
                    quantity: 1,
                }],
                duration_ms: 1000,
            },
            Recipe {
                id: "make-b".to_string(),
                facility: "assembler".to_string(),
                inputs: vec![ItemAmount {
                    item: "raw-b".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "item-b".to_string(),
                    quantity: 1,
                }],
                duration_ms: 1000,
            },
            Recipe {
                id: "make-c".to_string(),
                facility: "assembler".to_string(),
                inputs: vec![
                    ItemAmount {
                        item: "item-a".to_string(),
                        quantity: 1,
                    },
                    ItemAmount {
                        item: "item-b".to_string(),
                        quantity: 1,
                    },
                ],
                outputs: vec![ItemAmount {
                    item: "item-c".to_string(),
                    quantity: 1,
                }],
                duration_ms: 1000,
            },
        ],
    }
}

fn split_instance_chain_book() -> RecipeBook {
    RecipeBook {
        schema_version: 1,
        external_items: vec!["raw-material".to_string()],
        recipes: vec![
            Recipe {
                id: "make-intermediate".to_string(),
                facility: "fast-maker".to_string(),
                inputs: vec![ItemAmount {
                    item: "raw-material".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "intermediate-item".to_string(),
                    quantity: 1,
                }],
                duration_ms: 500,
            },
            Recipe {
                id: "make-finished".to_string(),
                facility: "slow-maker".to_string(),
                inputs: vec![ItemAmount {
                    item: "intermediate-item".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "finished-item".to_string(),
                    quantity: 1,
                }],
                duration_ms: 1000,
            },
        ],
    }
}

fn rate(numerator: i64, denominator: i64) -> Rate {
    Rate {
        numerator,
        denominator,
    }
}

fn throughput_request(item: &str, quantity: i64, duration_ms: i64) -> RecipeThroughputRequest {
    RecipeThroughputRequest {
        schema_version: SUPPORTED_THROUGHPUT_REQUEST_SCHEMA_VERSION,
        target: ThroughputTarget {
            item: item.to_string(),
            quantity,
            duration_ms,
        },
        external_inputs: Vec::new(),
    }
}

#[test]
fn accepts_valid_recipe_book() {
    let report = valid_book().validate();

    assert!(report.valid);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn promotes_valid_recipe_book() {
    let validated =
        ValidatedRecipeBook::try_from_recipe_book(valid_book()).expect("valid book should promote");

    assert_eq!(validated.recipe_book().schema_version, 1);
}

#[test]
fn rejects_invalid_ids() {
    let mut book = valid_book();
    book.recipes[0].id = "grind_originium_powder".to_string();
    book.recipes[0].facility = "Grinding Unit".to_string();
    book.recipes[0].outputs[0].item = "originium--powder".to_string();

    let report = book.validate();

    assert_codes(
        &report,
        &[
            "invalid-recipe-id",
            "invalid-facility-id",
            "invalid-item-id",
        ],
    );
}

#[test]
fn rejects_missing_input_links() {
    let mut book = valid_book();
    book.recipes[0].inputs[0].item = "missing-item".to_string();

    let report = book.validate();

    assert_codes(&report, &["missing-input-link"]);
}

#[test]
fn defers_ambiguous_output_producers_until_graph_resolution() {
    let mut book = valid_book();
    book.recipes.push(Recipe {
        id: "alternate-originium-powder".to_string(),
        facility: "grinding-unit".to_string(),
        inputs: vec![ItemAmount {
            item: "originium-ore".to_string(),
            quantity: 2,
        }],
        outputs: vec![ItemAmount {
            item: "originium-powder".to_string(),
            quantity: 3,
        }],
        duration_ms: 3000,
    });

    let validated = ValidatedRecipeBook::try_from_recipe_book(book)
        .expect("structurally valid alternate recipes should promote");
    let error = validated
        .resolve_graph("originium-powder")
        .expect_err("ambiguous producer should stop graph resolution");

    assert!(matches!(
        error,
        RecipeGraphError::AmbiguousProducer { item, recipes }
            if item == "originium-powder"
                && recipes == vec!["alternate-originium-powder", "grind-originium-powder"]
    ));
}

#[test]
fn defers_cycles_until_graph_resolution() {
    let book = RecipeBook {
        schema_version: 1,
        external_items: vec![],
        recipes: vec![
            Recipe {
                id: "make-a".to_string(),
                facility: "assembler".to_string(),
                inputs: vec![ItemAmount {
                    item: "item-b".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "item-a".to_string(),
                    quantity: 1,
                }],
                duration_ms: 1000,
            },
            Recipe {
                id: "make-b".to_string(),
                facility: "assembler".to_string(),
                inputs: vec![ItemAmount {
                    item: "item-a".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "item-b".to_string(),
                    quantity: 1,
                }],
                duration_ms: 1000,
            },
        ],
    };

    let validated = ValidatedRecipeBook::try_from_recipe_book(book)
        .expect("structurally valid cyclic recipes should promote");
    let error = validated
        .resolve_graph("item-a")
        .expect_err("encountered cycle should stop graph resolution");

    assert!(matches!(
        error,
        RecipeGraphError::RecipeCycle { recipes }
            if recipes == vec!["make-a", "make-b", "make-a"]
    ));
}

#[test]
fn rejects_non_positive_numbers() {
    let mut book = valid_book();
    book.recipes[0].inputs[0].quantity = 0;
    book.recipes[0].duration_ms = -1;

    let report = book.validate();

    assert_codes(&report, &["non-positive-quantity", "non-positive-duration"]);
}

#[test]
fn rejects_duplicate_recipe_side_items() {
    let mut book = valid_book();
    book.recipes[0].inputs.push(ItemAmount {
        item: "originium-ore".to_string(),
        quantity: 2,
    });
    book.recipes[0].outputs.push(ItemAmount {
        item: "originium-powder".to_string(),
        quantity: 2,
    });

    let report = book.validate();

    assert_codes(&report, &["duplicate-input-item", "duplicate-output-item"]);
}

#[test]
fn rejects_promotion_when_book_is_invalid() {
    let mut book = valid_book();
    book.recipes[0].inputs[0].item = "missing-item".to_string();

    let report = ValidatedRecipeBook::try_from_recipe_book(book)
        .expect_err("invalid book should not promote");

    assert_codes(&report, &["missing-input-link"]);
}

#[test]
fn resolves_multi_step_graph_dependency_first() {
    let graph = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .resolve_graph("originium-ingot")
        .expect("valid graph should resolve");

    assert_eq!(graph.target_item, "originium-ingot");
    assert_eq!(graph.external_items, ["originium-ore"]);
    assert_eq!(
        graph
            .recipes
            .iter()
            .map(|recipe| recipe.id.as_str())
            .collect::<Vec<_>>(),
        ["grind-originium-powder", "smelt-originium-ingot"]
    );
}

#[test]
fn resolves_external_target_without_recipes() {
    let graph = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .resolve_graph("originium-ore")
        .expect("external target should resolve");

    assert_eq!(graph.target_item, "originium-ore");
    assert_eq!(graph.external_items, ["originium-ore"]);
    assert!(graph.recipes.is_empty());
}

#[test]
fn rejects_unknown_graph_target() {
    let error = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .resolve_graph("missing-item")
        .expect_err("unknown target should fail");

    assert!(matches!(error, RecipeGraphError::UnknownTargetItem { .. }));
}

#[test]
fn calculates_single_step_throughput() {
    let report = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 2000));

    assert!(report.success);
    assert_eq!(report.target.expect("target should exist").rate, rate(1, 2));
    assert_eq!(report.recipe_rates.len(), 1);
    assert_eq!(
        report.recipe_rates[0].recipe,
        "grind-originium-powder".to_string()
    );
    assert_eq!(report.recipe_rates[0].runs_per_second, rate(1, 2));
    assert_eq!(report.recipe_rates[0].work_seconds_per_second, rate(1, 1));
    assert_eq!(
        report.recipe_rates[0].limiting_outputs,
        ["originium-powder"]
    );
    assert_eq!(report.external_input_rates[0].item, "originium-ore");
    assert_eq!(report.external_input_rates[0].rate, rate(1, 2));
}

#[test]
fn calculates_multi_step_throughput() {
    let report = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ingot", 1, 10000));

    assert!(report.success);
    assert_eq!(
        report
            .recipe_rates
            .iter()
            .map(|recipe_rate| recipe_rate.recipe.as_str())
            .collect::<Vec<_>>(),
        ["grind-originium-powder", "smelt-originium-ingot"]
    );
    assert_eq!(report.recipe_rates[0].runs_per_second, rate(1, 5));
    assert_eq!(report.recipe_rates[0].work_seconds_per_second, rate(2, 5));
    assert_eq!(report.recipe_rates[1].runs_per_second, rate(1, 10));
    assert_eq!(report.recipe_rates[1].work_seconds_per_second, rate(1, 2));
    assert_eq!(report.external_input_rates[0].rate, rate(1, 5));
}

#[test]
fn calculates_external_target_throughput() {
    let report = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ore", 3, 6000));

    assert!(report.success);
    assert!(report.recipe_rates.is_empty());
    assert_eq!(report.target.expect("target should exist").rate, rate(1, 2));
    assert_eq!(report.external_input_rates.len(), 1);
    assert_eq!(report.external_input_rates[0].item, "originium-ore");
    assert_eq!(report.external_input_rates[0].rate, rate(1, 2));
}

#[test]
fn handles_multi_output_shared_producer_without_double_counting() {
    let report = ValidatedRecipeBook::try_from_recipe_book(multi_output_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-core", 1, 1000));

    assert!(report.success);
    let split_rate = report
        .recipe_rates
        .iter()
        .find(|recipe_rate| recipe_rate.recipe == "split-originium-ore")
        .expect("split recipe should be present");

    assert_eq!(split_rate.runs_per_second, rate(1, 1));
    assert_eq!(
        split_rate.limiting_outputs,
        ["originium-powder", "originium-shard"]
    );
    assert_eq!(report.external_input_rates[0].rate, rate(1, 1));
    assert!(report.surplus_rates.is_empty());
}

#[test]
fn reports_multi_output_surplus() {
    let report = ValidatedRecipeBook::try_from_recipe_book(surplus_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 1000));

    assert!(report.success);
    assert_eq!(report.recipe_rates[0].runs_per_second, rate(1, 2));
    assert_eq!(report.surplus_rates.len(), 1);
    assert_eq!(report.surplus_rates[0].item, "originium-shard");
    assert_eq!(report.surplus_rates[0].rate, rate(1, 2));
}

#[test]
fn treats_request_external_inputs_as_recipe_graph_cutoffs() {
    let mut request = throughput_request("originium-ingot", 1, 1000);
    request.external_inputs = vec!["originium-powder".to_string()];

    let report = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&request);

    assert!(report.success);
    assert_eq!(report.recipe_rates.len(), 1);
    assert_eq!(report.recipe_rates[0].recipe, "smelt-originium-ingot");
    assert_eq!(report.external_input_rates.len(), 1);
    assert_eq!(report.external_input_rates[0].item, "originium-powder");
}

#[test]
fn rejects_invalid_duplicate_and_unknown_request_external_inputs() {
    let mut request = throughput_request("originium-ingot", 1, 1000);
    request.external_inputs = vec![
        "Bad_Input".to_string(),
        "originium-powder".to_string(),
        "originium-powder".to_string(),
    ];

    let diagnostics = validate_throughput_request(&request);
    assert_diagnostic_codes(
        &diagnostics,
        &["invalid-external-input-id", "duplicate-external-input"],
    );

    request.external_inputs = vec!["missing-item".to_string()];
    let report = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&request);
    assert!(!report.success);
    assert_diagnostic_codes(&report.diagnostics, &["unknown-external-input"]);
}

#[test]
fn rejects_unknown_throughput_request_fields_on_parse() {
    let error = serde_json::from_str::<RecipeThroughputRequest>(
        r#"{
          "schema_version": 2,
          "target": {
            "item": "originium-powder",
            "quantity": 1,
            "duration_ms": 1000,
            "extra": true
          },
          "external_inputs": []
        }"#,
    )
    .expect_err("unknown throughput request fields should be rejected");

    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn rejects_invalid_throughput_target_rate() {
    let report = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("Bad_Target", 0, -1));

    assert!(!report.success);
    assert_diagnostic_codes(
        &report.diagnostics,
        &[
            "invalid-target-id",
            "non-positive-target-quantity",
            "non-positive-target-duration",
        ],
    );
}

#[test]
fn rejects_unsupported_throughput_request_schema_version() {
    let mut request = throughput_request("originium-powder", 1, 1000);
    request.schema_version = SUPPORTED_THROUGHPUT_REQUEST_SCHEMA_VERSION + 1;

    let report = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&request);

    assert!(!report.success);
    assert_diagnostic_codes(
        &report.diagnostics,
        &["unsupported-throughput-request-schema-version"],
    );
}

#[test]
fn rejects_unknown_throughput_target() {
    let report = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("missing-item", 1, 1000));

    assert!(!report.success);
    assert_diagnostic_codes(&report.diagnostics, &["unknown-target-item"]);
}

#[test]
fn calculates_single_recipe_facility_requirement() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 2000));

    let report = calculate_facility_requirements(&throughput);

    assert!(report.success);
    assert_eq!(report.recipe_requirements.len(), 1);
    assert_eq!(report.recipe_requirements[0].required_facilities, 1);
    assert_eq!(report.recipe_requirements[0].unused_capacity, rate(0, 1));
    assert_eq!(report.facility_summaries[0].facility, "grinding-unit");
    assert_eq!(report.facility_summaries[0].required_facilities, 1);
}

#[test]
fn rounds_work_rate_above_one_up_to_multiple_facilities() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 4, 3000));

    let report = calculate_facility_requirements(&throughput);

    assert!(report.success);
    assert_eq!(
        report.recipe_requirements[0].work_seconds_per_second,
        rate(8, 3)
    );
    assert_eq!(report.recipe_requirements[0].required_facilities, 3);
    assert_eq!(report.recipe_requirements[0].unused_capacity, rate(1, 3));
}

#[test]
fn external_target_requires_no_facilities() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ore", 1, 1000));

    let report = calculate_facility_requirements(&throughput);

    assert!(report.success);
    assert!(report.recipe_requirements.is_empty());
    assert!(report.facility_summaries.is_empty());
}

#[test]
fn aggregates_facility_summaries_without_recipe_sharing() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(same_facility_chain_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("item-c", 1, 1000));

    let report = calculate_facility_requirements(&throughput);

    assert!(report.success);
    assert_eq!(report.recipe_requirements.len(), 3);
    assert_eq!(report.facility_summaries.len(), 1);
    assert_eq!(report.facility_summaries[0].facility, "assembler");
    assert_eq!(report.facility_summaries[0].required_facilities, 3);
    assert_eq!(report.facility_summaries[0].unused_capacity, rate(0, 1));
}

#[test]
fn failed_throughput_input_becomes_facility_failure_report() {
    let throughput = RecipeThroughputReport::failure(ThroughputDiagnostic::error(
        "unknown-target-item",
        "/target/item",
        Some("missing-item".to_string()),
        "target item is unknown",
    ));

    let report = calculate_facility_requirements(&throughput);

    assert!(!report.success);
    assert!(report.recipe_requirements.is_empty());
    assert!(report.facility_summaries.is_empty());
    assert_facility_diagnostic_codes(&report.diagnostics, &["upstream-throughput-failed"]);
}

#[test]
fn builds_multi_step_recipe_wiring_graph() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ingot", 1, 10000));

    let report = build_recipe_wiring_graph(&throughput);

    assert!(report.success);
    assert_eq!(
        report.nodes.iter().map(wiring_node_id).collect::<Vec<_>>(),
        [
            "external:originium-ore",
            "recipe:grind-originium-powder",
            "recipe:smelt-originium-ingot",
            "target:originium-ingot"
        ]
    );
    assert_eq!(
        report
            .edges
            .iter()
            .map(|edge| {
                (
                    edge.source.as_str(),
                    edge.target.as_str(),
                    edge.kind.as_str(),
                    edge.item.as_str(),
                    edge.rate,
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                "external:originium-ore",
                "recipe:grind-originium-powder",
                "external-input",
                "originium-ore",
                rate(1, 5)
            ),
            (
                "recipe:grind-originium-powder",
                "recipe:smelt-originium-ingot",
                "recipe-flow",
                "originium-powder",
                rate(1, 5)
            ),
            (
                "recipe:smelt-originium-ingot",
                "target:originium-ingot",
                "target-output",
                "originium-ingot",
                rate(1, 10)
            )
        ]
    );
}

#[test]
fn builds_external_target_wiring_graph() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ore", 1, 1000));

    let report = build_recipe_wiring_graph(&throughput);

    assert!(report.success);
    assert_eq!(
        report.nodes.iter().map(wiring_node_id).collect::<Vec<_>>(),
        ["external:originium-ore", "target:originium-ore"]
    );
    assert_eq!(report.edges.len(), 1);
    assert_eq!(report.edges[0].source, "external:originium-ore");
    assert_eq!(report.edges[0].target, "target:originium-ore");
    assert_eq!(report.edges[0].kind, "target-output");
}

#[test]
fn builds_surplus_wiring_edge() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(surplus_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 1000));

    let report = build_recipe_wiring_graph(&throughput);

    assert!(report.success);
    let surplus_edge = report
        .edges
        .iter()
        .find(|edge| edge.kind == "surplus-output")
        .expect("surplus edge should be present");

    assert_eq!(surplus_edge.source, "recipe:split-originium-ore");
    assert_eq!(surplus_edge.target, "surplus:originium-shard");
    assert_eq!(surplus_edge.item, "originium-shard");
    assert_eq!(surplus_edge.rate, rate(1, 2));
}

#[test]
fn serializes_recipe_wiring_nodes_as_discriminated_union() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(surplus_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 1000));

    let report = build_recipe_wiring_graph(&throughput);

    assert!(report.success);
    assert!(matches!(
        &report.nodes[0],
        RecipeWiringGraphNode::External { id, item }
            if id == "external:originium-ore" && item == "originium-ore"
    ));
    assert!(matches!(
        &report.nodes[1],
        RecipeWiringGraphNode::Recipe { id, recipe }
            if id == "recipe:split-originium-ore" && recipe == "split-originium-ore"
    ));
    assert!(matches!(
        &report.nodes[2],
        RecipeWiringGraphNode::Target { id, item }
            if id == "target:originium-powder" && item == "originium-powder"
    ));
    assert!(matches!(
        &report.nodes[3],
        RecipeWiringGraphNode::Surplus { id, item }
            if id == "surplus:originium-shard" && item == "originium-shard"
    ));

    let json = serde_json::to_value(&report).expect("wiring report should serialize");
    let nodes = json.get("nodes").expect("nodes should exist");
    assert_eq!(
        nodes,
        &serde_json::json!([
            {
                "kind": "external",
                "id": "external:originium-ore",
                "item": "originium-ore"
            },
            {
                "kind": "recipe",
                "id": "recipe:split-originium-ore",
                "recipe": "split-originium-ore"
            },
            {
                "kind": "target",
                "id": "target:originium-powder",
                "item": "originium-powder"
            },
            {
                "kind": "surplus",
                "id": "surplus:originium-shard",
                "item": "originium-shard"
            }
        ])
    );
    let nodes = nodes.as_array().expect("nodes should be an array");
    assert!(nodes.iter().all(|node| {
        !node
            .as_object()
            .expect("node should be an object")
            .values()
            .any(|value| value.is_null())
    }));
}

#[test]
fn builds_logical_instance_wiring_for_multi_step_chain() {
    let report = build_instance_wiring_report(
        multi_step_book(),
        throughput_request("originium-ingot", 1, 10000),
    );

    assert!(report.success);
    assert_eq!(
        report
            .nodes
            .iter()
            .map(instance_wiring_node_id)
            .collect::<Vec<_>>(),
        [
            "external:originium-ore",
            "facility-instance:grind-originium-powder:0",
            "facility-instance:smelt-originium-ingot:0",
            "target:originium-ingot"
        ]
    );
    assert_eq!(
        report
            .edges
            .iter()
            .map(instance_wiring_edge_tuple)
            .collect::<Vec<_>>(),
        [
            (
                "external:originium-ore",
                "facility-instance:grind-originium-powder:0",
                "external-input",
                "originium-ore",
                rate(1, 5)
            ),
            (
                "facility-instance:grind-originium-powder:0",
                "facility-instance:smelt-originium-ingot:0",
                "recipe-flow",
                "originium-powder",
                rate(1, 5)
            ),
            (
                "facility-instance:smelt-originium-ingot:0",
                "target:originium-ingot",
                "target-output",
                "originium-ingot",
                rate(1, 10)
            )
        ]
    );
}

#[test]
fn keeps_external_target_as_logical_instance_wiring_endpoint() {
    let report =
        build_instance_wiring_report(valid_book(), throughput_request("originium-ore", 1, 1000));

    assert!(report.success);
    assert_eq!(
        report
            .nodes
            .iter()
            .map(instance_wiring_node_id)
            .collect::<Vec<_>>(),
        ["external:originium-ore", "target:originium-ore"]
    );
    assert_eq!(
        report
            .edges
            .iter()
            .map(instance_wiring_edge_tuple)
            .collect::<Vec<_>>(),
        [(
            "external:originium-ore",
            "target:originium-ore",
            "target-output",
            "originium-ore",
            rate(1, 1)
        )]
    );
}

#[test]
fn splits_edges_across_source_and_target_recipe_instances() {
    let report = build_instance_wiring_report(
        split_instance_chain_book(),
        throughput_request("finished-item", 3, 1000),
    );

    assert!(report.success);
    assert_eq!(
        report
            .nodes
            .iter()
            .filter(|node| matches!(node, FacilityInstanceWiringNode::Facility { .. }))
            .count(),
        5
    );
    let first_source_instance = report
        .nodes
        .iter()
        .find(|node| {
            matches!(
                node,
                FacilityInstanceWiringNode::Facility { id, .. }
                    if id == "facility-instance:make-intermediate:0"
            )
        })
        .expect("first source facility instance should exist");
    assert!(matches!(
        first_source_instance,
        FacilityInstanceWiringNode::Facility {
            runs_per_second,
            work_seconds_per_second,
            unused_capacity,
            ..
        } if *runs_per_second == rate(3, 2)
            && *work_seconds_per_second == rate(3, 4)
            && *unused_capacity == rate(1, 4)
    ));

    let recipe_flow_edges = report
        .edges
        .iter()
        .filter(|edge| edge.kind == "recipe-flow")
        .map(instance_wiring_edge_tuple)
        .collect::<Vec<_>>();

    assert_eq!(
        recipe_flow_edges,
        [
            (
                "facility-instance:make-intermediate:0",
                "facility-instance:make-finished:0",
                "recipe-flow",
                "intermediate-item",
                rate(1, 1)
            ),
            (
                "facility-instance:make-intermediate:0",
                "facility-instance:make-finished:1",
                "recipe-flow",
                "intermediate-item",
                rate(1, 2)
            ),
            (
                "facility-instance:make-intermediate:1",
                "facility-instance:make-finished:1",
                "recipe-flow",
                "intermediate-item",
                rate(1, 2)
            ),
            (
                "facility-instance:make-intermediate:1",
                "facility-instance:make-finished:2",
                "recipe-flow",
                "intermediate-item",
                rate(1, 1)
            )
        ]
    );
}

#[test]
fn builds_surplus_instance_wiring_edge() {
    let report = build_instance_wiring_report(
        surplus_book(),
        throughput_request("originium-powder", 1, 1000),
    );

    assert!(report.success);
    let surplus_edge = report
        .edges
        .iter()
        .find(|edge| edge.kind == "surplus-output")
        .expect("surplus edge should be present");

    assert_eq!(
        surplus_edge.source,
        "facility-instance:split-originium-ore:0"
    );
    assert_eq!(surplus_edge.target, "surplus:originium-shard");
    assert_eq!(surplus_edge.item, "originium-shard");
    assert_eq!(surplus_edge.rate, rate(1, 2));
}

#[test]
fn mismatched_successful_upstream_reports_fail_instance_wiring() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ingot", 1, 10000));
    let mut facilities = calculate_facility_requirements(&throughput);
    let recipe_wiring = build_recipe_wiring_graph(&throughput);
    facilities.recipe_requirements[0].facility = "other-facility".to_string();

    let report = build_facility_instance_wiring(&throughput, &facilities, &recipe_wiring);

    assert!(!report.success);
    assert_instance_wiring_diagnostic_codes(&report.diagnostics, &["facility-id-mismatch"]);
}

#[test]
fn malformed_successful_edge_kind_fails_instance_wiring() {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ingot", 1, 10000));
    let facilities = calculate_facility_requirements(&throughput);
    let mut recipe_wiring = build_recipe_wiring_graph(&throughput);
    recipe_wiring.edges[0].kind = "recipe-flow".to_string();

    let report = build_facility_instance_wiring(&throughput, &facilities, &recipe_wiring);

    assert!(!report.success);
    assert_instance_wiring_diagnostic_codes(&report.diagnostics, &["edge-kind-mismatch"]);
}

#[test]
fn malformed_successful_throughput_without_target_fails_instance_wiring() {
    let mut throughput = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 1000));
    let facilities = calculate_facility_requirements(&throughput);
    let recipe_wiring = build_recipe_wiring_graph(&throughput);
    throughput.target = None;

    let report = build_facility_instance_wiring(&throughput, &facilities, &recipe_wiring);

    assert!(!report.success);
    assert_instance_wiring_diagnostic_codes(&report.diagnostics, &["missing-target"]);
}

#[test]
fn failed_upstream_reports_become_instance_wiring_failure_reports() {
    let failed_throughput = RecipeThroughputReport::failure(ThroughputDiagnostic::error(
        "unknown-target-item",
        "/target/item",
        Some("missing-item".to_string()),
        "target item is unknown",
    ));
    let failed_facilities = calculate_facility_requirements(&failed_throughput);
    let failed_wiring = build_recipe_wiring_graph(&failed_throughput);
    let report =
        build_facility_instance_wiring(&failed_throughput, &failed_facilities, &failed_wiring);
    assert_instance_wiring_diagnostic_codes(&report.diagnostics, &["upstream-throughput-failed"]);

    let throughput = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 1000));
    let mut facilities = calculate_facility_requirements(&throughput);
    facilities.success = false;
    let recipe_wiring = build_recipe_wiring_graph(&throughput);
    let report = build_facility_instance_wiring(&throughput, &facilities, &recipe_wiring);
    assert_instance_wiring_diagnostic_codes(
        &report.diagnostics,
        &["upstream-facility-requirements-failed"],
    );

    let facilities = calculate_facility_requirements(&throughput);
    let mut recipe_wiring = build_recipe_wiring_graph(&throughput);
    recipe_wiring.success = false;
    let report = build_facility_instance_wiring(&throughput, &facilities, &recipe_wiring);
    assert_instance_wiring_diagnostic_codes(
        &report.diagnostics,
        &["upstream-recipe-wiring-failed"],
    );
}

#[test]
fn failed_throughput_input_becomes_wiring_failure_report() {
    let throughput = RecipeThroughputReport::failure(ThroughputDiagnostic::error(
        "unknown-target-item",
        "/target/item",
        Some("missing-item".to_string()),
        "target item is unknown",
    ));

    let report = build_recipe_wiring_graph(&throughput);

    assert!(!report.success);
    assert!(report.nodes.is_empty());
    assert!(report.edges.is_empty());
    assert_wiring_diagnostic_codes(&report.diagnostics, &["upstream-throughput-failed"]);
}

#[test]
fn malformed_successful_throughput_without_target_fails_wiring() {
    let mut throughput = ValidatedRecipeBook::try_from_recipe_book(valid_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 1000));
    throughput.target = None;

    let report = build_recipe_wiring_graph(&throughput);

    assert!(!report.success);
    assert_wiring_diagnostic_codes(&report.diagnostics, &["missing-target"]);
}

#[test]
fn malformed_successful_throughput_without_producer_fails_wiring() {
    let mut throughput = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ingot", 1, 10000));
    throughput.recipe_rates[0].output_rates.clear();

    let report = build_recipe_wiring_graph(&throughput);

    assert!(!report.success);
    assert_wiring_diagnostic_codes(&report.diagnostics, &["missing-producer"]);
}

#[test]
fn malformed_successful_throughput_with_ambiguous_producer_fails_wiring() {
    let mut throughput = ValidatedRecipeBook::try_from_recipe_book(multi_step_book())
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-ingot", 1, 10000));
    throughput.recipe_rates[1].output_rates.push(ItemRate {
        item: "originium-powder".to_string(),
        rate: rate(1, 10),
    });

    let report = build_recipe_wiring_graph(&throughput);

    assert!(!report.success);
    assert_wiring_diagnostic_codes(&report.diagnostics, &["ambiguous-producer"]);
}

#[test]
fn rejects_unknown_recipe_fields_on_parse() {
    let error = serde_json::from_str::<RecipeBook>(
        r#"{
          "schema_version": 1,
          "external_items": ["originium-ore"],
          "recipes": [
            {
              "id": "grind-originium-powder",
              "facility": "grinding-unit",
              "inputs": [],
              "outputs": [{"item": "originium-powder", "quantity": 1}],
              "duration_ms": 2000,
              "extra": true
            }
          ]
        }"#,
    )
    .expect_err("unknown recipe fields should be rejected");

    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn rejects_unknown_item_amount_fields_on_parse() {
    let error = serde_json::from_str::<RecipeBook>(
        r#"{
          "schema_version": 1,
          "external_items": ["originium-ore"],
          "recipes": [
            {
              "id": "grind-originium-powder",
              "facility": "grinding-unit",
              "inputs": [{"item": "originium-ore", "quantity": 1, "extra": true}],
              "outputs": [{"item": "originium-powder", "quantity": 1}],
              "duration_ms": 2000
            }
          ]
        }"#,
    )
    .expect_err("unknown item amount fields should be rejected");

    assert!(error.to_string().contains("unknown field"));
}

fn assert_codes(report: &ValidationReport, expected_codes: &[&str]) {
    assert!(!report.valid);

    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    for expected_code in expected_codes {
        assert!(
            codes.contains(expected_code),
            "expected diagnostic code '{expected_code}', got {codes:?}"
        );
    }
}

fn assert_diagnostic_codes(diagnostics: &[ThroughputDiagnostic], expected_codes: &[&str]) {
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    for expected_code in expected_codes {
        assert!(
            codes.contains(expected_code),
            "expected diagnostic code '{expected_code}', got {codes:?}"
        );
    }
}

fn assert_facility_diagnostic_codes(
    diagnostics: &[FacilityRequirementDiagnostic],
    expected_codes: &[&str],
) {
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    for expected_code in expected_codes {
        assert!(
            codes.contains(expected_code),
            "expected diagnostic code '{expected_code}', got {codes:?}"
        );
    }
}

fn assert_wiring_diagnostic_codes(
    diagnostics: &[RecipeWiringGraphDiagnostic],
    expected_codes: &[&str],
) {
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    for expected_code in expected_codes {
        assert!(
            codes.contains(expected_code),
            "expected diagnostic code '{expected_code}', got {codes:?}"
        );
    }
}

fn assert_instance_wiring_diagnostic_codes(
    diagnostics: &[FacilityInstanceWiringDiagnostic],
    expected_codes: &[&str],
) {
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    for expected_code in expected_codes {
        assert!(
            codes.contains(expected_code),
            "expected diagnostic code '{expected_code}', got {codes:?}"
        );
    }
}

fn wiring_node_id(node: &RecipeWiringGraphNode) -> &str {
    match node {
        RecipeWiringGraphNode::External { id, .. }
        | RecipeWiringGraphNode::Recipe { id, .. }
        | RecipeWiringGraphNode::Target { id, .. }
        | RecipeWiringGraphNode::Surplus { id, .. } => id,
    }
}

fn instance_wiring_node_id(node: &FacilityInstanceWiringNode) -> &str {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. }
        | FacilityInstanceWiringNode::External { id, .. }
        | FacilityInstanceWiringNode::Target { id, .. }
        | FacilityInstanceWiringNode::Surplus { id, .. } => id,
    }
}

fn instance_wiring_edge_tuple(edge: &FacilityInstanceWiringEdge) -> (&str, &str, &str, &str, Rate) {
    (
        edge.source.as_str(),
        edge.target.as_str(),
        edge.kind.as_str(),
        edge.item.as_str(),
        edge.rate,
    )
}

fn build_instance_wiring_report(
    book: RecipeBook,
    request: RecipeThroughputRequest,
) -> FacilityInstanceWiringReport {
    let throughput = ValidatedRecipeBook::try_from_recipe_book(book)
        .expect("valid book should promote")
        .calculate_throughput(&request);
    let facilities = calculate_facility_requirements(&throughput);
    let recipe_wiring = build_recipe_wiring_graph(&throughput);

    build_facility_instance_wiring(&throughput, &facilities, &recipe_wiring)
}
