mod facilities;
mod graph;
mod id;
mod index;
mod instances;
mod load;
mod model;
mod production_graph;
mod selection;
mod source_plan;
mod throughput;
mod validate;
mod validated;
mod wiring;

pub use facilities::{
    ContextualFacilityRequirement, ContextualFacilityRequirementReport,
    FacilityRequirementDiagnostic, FacilityRequirementReport, FacilityRequirementSummary,
    RecipeFacilityRequirement, calculate_contextual_facility_requirements,
    calculate_facility_requirements,
};
pub use graph::RecipeGraphError;
pub use id::validate_target_item_id;
pub use instances::{
    FacilityInstanceWiringDiagnostic, FacilityInstanceWiringEdge, FacilityInstanceWiringNode,
    FacilityInstanceWiringReport, build_contextual_facility_instance_wiring,
    build_facility_instance_wiring,
};
pub use load::{LoadRecipeBookError, load_recipe_book};
pub use model::{ItemAmount, Recipe, RecipeBook, RecipeGraph};
pub use production_graph::{
    ContextualProductionEdge, ContextualProductionFlowKind, ContextualProductionGraphDiagnostic,
    ContextualProductionGraphReport, ContextualProductionNode, build_contextual_production_graph,
};
pub use selection::{
    RecipeSelectionCheckReport, RecipeSelectionCheckStatus, RecipeSelectionDiagnostic,
    check_recipe_selections,
};
pub use source_plan::{
    RecipeSource, RecipeSourceCheckReport, RecipeSourceCheckStatus, RecipeSourceDiagnostic,
    RecipeSourceGroup, RecipeSourceNode, RecipeSourcePlanRequest, RecipeSourceResolution,
    RecipeSourceSelection, SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION, check_recipe_source_plan,
};
pub use throughput::{
    ContextualExternalInputRate, ContextualMaterialFlowRate, ContextualRecipeRunRate,
    ContextualSurplusRate, ContextualThroughputDiagnostic, ContextualThroughputReport, ItemRate,
    Rate, RecipeProducerSelection, RecipeProducerSelectionGroup, RecipeRunRate,
    RecipeThroughputReport, RecipeThroughputRequest, SUPPORTED_THROUGHPUT_REQUEST_SCHEMA_VERSION,
    ThroughputDiagnostic, ThroughputTarget, validate_throughput_request,
};
pub use validate::{
    SUPPORTED_SCHEMA_VERSION, ValidationDiagnostic, ValidationReport, validate_recipe_book,
};
pub use validated::ValidatedRecipeBook;
pub use wiring::{
    RecipeWiringEdge, RecipeWiringGraphDiagnostic, RecipeWiringGraphNode, RecipeWiringGraphReport,
    build_recipe_wiring_graph,
};

#[cfg(test)]
mod tests;
