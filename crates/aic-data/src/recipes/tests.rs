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

fn rate(numerator: i64, denominator: i64) -> Rate {
    Rate {
        numerator,
        denominator,
    }
}

fn throughput_request(item: &str, quantity: i64, duration_ms: i64) -> RecipeThroughputRequest {
    RecipeThroughputRequest {
        schema_version: 1,
        target: ThroughputTarget {
            item: item.to_string(),
            quantity,
            duration_ms,
        },
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
fn rejects_ambiguous_output_producers() {
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

    let report = book.validate();

    assert_codes(&report, &["ambiguous-output-producer"]);
}

#[test]
fn rejects_cycles() {
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

    let report = book.validate();

    assert_codes(&report, &["recipe-cycle"]);
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
    let book = RecipeBook {
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
    };

    let report = ValidatedRecipeBook::try_from_recipe_book(book)
        .expect("valid book should promote")
        .calculate_throughput(&throughput_request("originium-powder", 1, 1000));

    assert!(report.success);
    assert_eq!(report.recipe_rates[0].runs_per_second, rate(1, 2));
    assert_eq!(report.surplus_rates.len(), 1);
    assert_eq!(report.surplus_rates[0].item, "originium-shard");
    assert_eq!(report.surplus_rates[0].rate, rate(1, 2));
}

#[test]
fn rejects_unknown_throughput_request_fields_on_parse() {
    let error = serde_json::from_str::<RecipeThroughputRequest>(
        r#"{
          "schema_version": 1,
          "target": {
            "item": "originium-powder",
            "quantity": 1,
            "duration_ms": 1000,
            "extra": true
          }
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
