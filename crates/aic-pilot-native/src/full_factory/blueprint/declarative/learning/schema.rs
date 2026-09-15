//! Packed physical columns. Absent values are zero; kind/relation bits define applicability.
//! Positions and lengths are in occupied-span units. Rates are log1p(items/second).
pub const NODE_STATIC: usize = 31;
pub const NODE_STATE: usize = 10;
pub const EDGE_STATIC: usize = 33;
pub const EDGE_STATE: usize = 4;
pub const PATH_FEATURES: usize = 8;
pub const GLOBAL_FEATURES: usize = 15;
pub const ACTION_FEATURES: usize = 12;
pub const PORT_FEATURES: usize = 11;
pub mod node {
    pub const FACILITY: usize = 0;
    pub const TERMINAL: usize = 1;
    pub const EXTERNAL: usize = 2;
    pub const CHANNEL: usize = 3;
    pub const COMPONENT: usize = 4;
    pub const PORT: usize = 5;
    pub const NETWORK: usize = 6;
    pub const COMMODITY: usize = 7;
    pub const WIDTH: usize = 8;
    pub const HEIGHT: usize = 9;
    pub const ROTATIONS: usize = 10;
    pub const TRANSPORT: usize = 14;
    pub const DIRECTION: usize = 16;
    pub const DEMAND: usize = 18;
    pub const CAPACITY: usize = 19;
    pub const COMPONENT_KIND: usize = 20;
    pub const INPUT_MASK: usize = 23;
    pub const OUTPUT_MASK: usize = 27;
}
pub mod state {
    pub const X: usize = 0;
    pub const Y: usize = 1;
    pub const WIDTH: usize = 2;
    pub const HEIGHT: usize = 3;
    pub const SIN: usize = 4;
    pub const COS: usize = 5;
    pub const NORMAL_X: usize = 6;
    pub const NORMAL_Y: usize = 7;
    pub const CONFLICTS: usize = 8;
    pub const SELECTED: usize = 9;
}
pub mod edge {
    pub const TRANSPORT: usize = 0;
    pub const FACILITY_TERMINAL: usize = 1;
    pub const FACILITY_PORT: usize = 2;
    pub const SELECTED_PORT: usize = 3;
    pub const ORIGINAL_PORT: usize = 4;
    pub const COMPONENT_CHANNEL: usize = 5;
    pub const NETWORK_MEMBER: usize = 6;
    pub const COMMODITY_MEMBER: usize = 7;
    pub const MEDIUM: usize = 8;
    pub const RATE: usize = 10;
    pub const CAPACITY: usize = 11;
    pub const CURRENT_FIRST: usize = 12;
    pub const CURRENT_LAST: usize = 17;
    pub const BASELINE_FIRST: usize = 22;
    pub const BASELINE_LAST: usize = 27;
    pub const ZERO_LENGTH: usize = 32;
}
/// Capacity-aware allocation accounting shared by collection and replay.
pub fn row_bytes<T>(rows: &Vec<T>, allocated: bool) -> usize {
    (if allocated {
        rows.capacity()
    } else {
        rows.len()
    }) * std::mem::size_of::<T>()
}
/// Serde's built-in arrays stop at 32; retain typed 33-column native rows.
pub(super) mod rows {
    use serde::{
        Deserialize, Deserializer, Serializer,
        de::{Error, SeqAccess, Visitor},
        ser::SerializeSeq,
    };
    use std::fmt;
    pub fn serialize<S: Serializer, const N: usize>(
        v: &[[f32; N]],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(v.len()))?;
        for row in v {
            seq.serialize_element(row.as_slice())?;
        }
        seq.end()
    }
    struct Row<const N: usize>([f32; N]);
    impl<'de, const N: usize> Deserialize<'de> for Row<N> {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct RowVisitor<const N: usize>;
            impl<'de, const N: usize> Visitor<'de> for RowVisitor<N> {
                type Value = Row<N>;
                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    write!(f, "{N} physical columns")
                }
                fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                    let mut row = [0.; N];
                    for (i, value) in row.iter_mut().enumerate() {
                        *value = seq
                            .next_element()?
                            .ok_or_else(|| A::Error::invalid_length(i, &self))?;
                    }
                    if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                        return Err(A::Error::invalid_length(N + 1, &self));
                    }
                    Ok(Row(row))
                }
            }
            d.deserialize_seq(RowVisitor::<N>)
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        d: D,
    ) -> Result<Vec<[f32; N]>, D::Error> {
        Ok(Vec::<Row<N>>::deserialize(d)?
            .into_iter()
            .map(|r| r.0)
            .collect())
    }
}
/// Global array offsets. All logarithms here are ln(1+x), including zero scores/work.
pub mod global {
    pub const BEST_RATIOS: usize = 0;
    pub const INITIAL_SCORES: usize = 3;
    pub const PENDING: usize = 6;
    pub const CONFLICT_AREA: usize = 7;
    pub const REMAINING_DECISIONS: usize = 8;
    pub const REMAINING_WORK: usize = 9;
    pub const NEXT_WORK: usize = 10;
    pub const PADDING: usize = 11;
    pub const WIDTH: usize = 12;
    pub const HEIGHT: usize = 13;
    pub const CELL_UNIT: usize = 14;
}
pub mod port {
    pub const X: usize = 0;
    pub const Y: usize = 1;
    pub const NORMAL_X: usize = 2;
    pub const NORMAL_Y: usize = 3;
    pub const INPUT: usize = 4;
    pub const OUTPUT: usize = 5;
    pub const BELT: usize = 6;
    pub const PIPE: usize = 7;
    pub const CURRENT: usize = 8;
    pub const ORIGINAL: usize = 9;
    pub const CAPACITY: usize = 10;
}
pub mod path {
    pub const X: usize = 0;
    pub const Y: usize = 1;
    pub const IN_X: usize = 2;
    pub const IN_Y: usize = 3;
    pub const OUT_X: usize = 4;
    pub const OUT_Y: usize = 5;
    pub const START: usize = 6;
    pub const END: usize = 7;
}
