use std::collections::BTreeMap;

use super::{
    FacilityCatalog, FacilityCatalogValidationReport, FacilityDefinition, validate_facility_catalog,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedFacilityCatalog {
    catalog: FacilityCatalog,
    facility_index: BTreeMap<String, usize>,
}

impl ValidatedFacilityCatalog {
    pub fn try_from_catalog(
        catalog: FacilityCatalog,
    ) -> Result<Self, FacilityCatalogValidationReport> {
        let report = validate_facility_catalog(&catalog);
        if !report.valid {
            return Err(report);
        }

        let facility_index = catalog
            .facilities
            .iter()
            .enumerate()
            .map(|(index, facility)| (facility.id.clone(), index))
            .collect();

        Ok(Self {
            catalog,
            facility_index,
        })
    }

    pub fn catalog(&self) -> &FacilityCatalog {
        &self.catalog
    }

    pub fn facility(&self, facility_id: &str) -> Option<&FacilityDefinition> {
        self.facility_index
            .get(facility_id)
            .map(|index| &self.catalog.facilities[*index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityDefinition, FacilityFootprint};

    fn catalog_with_facility(id: &str) -> FacilityCatalog {
        FacilityCatalog {
            schema_version: 2,
            facilities: vec![FacilityDefinition {
                id: id.to_string(),
                footprint: FacilityFootprint {
                    width: 3,
                    height: 2,
                },
                allowed_rotations: vec![0, 90, 180, 270],
            }],
        }
    }

    #[test]
    fn promotes_valid_catalog_and_indexes_facilities() {
        let validated =
            ValidatedFacilityCatalog::try_from_catalog(catalog_with_facility("grinding-unit"))
                .expect("valid catalog should promote");

        assert_eq!(validated.catalog().facilities.len(), 1);
        assert_eq!(
            validated
                .facility("grinding-unit")
                .expect("facility should be indexed")
                .footprint,
            FacilityFootprint {
                width: 3,
                height: 2,
            }
        );
        assert!(validated.facility("missing-unit").is_none());
    }

    #[test]
    fn rejects_invalid_catalog_promotion() {
        let report =
            ValidatedFacilityCatalog::try_from_catalog(catalog_with_facility("Invalid Facility"))
                .expect_err("invalid catalog must not promote");

        assert!(!report.valid);
        assert_eq!(report.diagnostics[0].code, "invalid-facility-id");
    }
}
