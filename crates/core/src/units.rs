//! Unit handling. Internally weights are stored in kilograms and distances in metres.

use serde::{Deserialize, Serialize};

pub const KG_PER_LB: f64 = 0.453_592_37;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeightUnit {
    Kg,
    Lb,
}

impl WeightUnit {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "kg" | "kgs" => Some(Self::Kg),
            "lb" | "lbs" => Some(Self::Lb),
            _ => None,
        }
    }

    pub fn to_kg(self, v: f64) -> f64 {
        match self {
            Self::Kg => v,
            Self::Lb => v * KG_PER_LB,
        }
    }

    pub fn from_kg(self, kg: f64) -> f64 {
        match self {
            Self::Kg => kg,
            Self::Lb => kg / KG_PER_LB,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DistanceUnit {
    M,
    Km,
    Ft,
    Mi,
}

impl DistanceUnit {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "m" | "meters" | "metres" => Some(Self::M),
            "km" | "kms" => Some(Self::Km),
            "ft" | "feet" => Some(Self::Ft),
            "mi" | "mile" | "miles" => Some(Self::Mi),
            _ => None,
        }
    }

    pub fn to_m(self, v: f64) -> f64 {
        match self {
            Self::M => v,
            Self::Km => v * 1000.0,
            Self::Ft => v * 0.3048,
            Self::Mi => v * 1609.344,
        }
    }
}
