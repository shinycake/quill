//! Affiliate programs of mini apps (TDLib `affiliateProgram*`), after
//! tdesktop `info/bot/starref/*`: a bot owner sets the commission and
//! duration; users, their bots and their channels join programs and
//! share referral links.

use super::{int, text};
use crate::premium_hub::StarAmount;
use serde_json::{Value, json};

/// tdesktop's default commission for a new program, in permille.
pub const COMMISSION_DEFAULT: i32 = 200;
/// tdesktop's default duration for a new program, in months.
pub const DURATION_DEFAULT: i32 = 12;
/// The duration stops tdesktop offers (999 = forever).
pub const DURATION_STOPS: [i32; 7] = [1, 3, 6, 12, 24, 36, super::DURATION_FOREVER];
/// Programs per `searchAffiliatePrograms` / connected page (tdesktop
/// `kPerPage`; TDLib has no option for it).
pub const PROGRAMS_PER_PAGE: i32 = 50;

/// `affiliateProgramParameters`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProgramParams {
    /// Commission in permille (1‰ = 0.1%).
    pub commission_per_mille: i32,
    /// Months the affiliate earns; 0 = forever.
    pub month_count: i32,
}

impl ProgramParams {
    pub fn parse(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        Self {
            commission_per_mille: int(value.get("commission_per_mille")) as i32,
            month_count: int(value.get("month_count")) as i32,
        }
    }

    pub fn to_json(self) -> Value {
        json!({
            "@type": "affiliateProgramParameters",
            "commission_per_mille": self.commission_per_mille,
            "month_count": self.month_count,
        })
    }
}

/// `affiliateProgramInfo`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProgramInfo {
    pub params: ProgramParams,
    /// When an ended program stops; 0 while it runs.
    pub end_date: i32,
    pub daily_revenue_per_user: StarAmount,
}

impl ProgramInfo {
    pub fn parse(value: Option<&Value>) -> Option<Self> {
        let value = value.filter(|v| !v.is_null())?;
        Some(Self {
            params: ProgramParams::parse(value.get("parameters")),
            end_date: int(value.get("end_date")) as i32,
            daily_revenue_per_user: StarAmount::parse(value.get("daily_revenue_per_user_amount")),
        })
    }

    /// A running program (tdesktop `State::exists`).
    pub fn is_running(&self) -> bool {
        self.params.commission_per_mille > 0 && self.end_date == 0
    }
}

/// `foundAffiliateProgram`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoundProgram {
    pub bot_user_id: i64,
    pub info: ProgramInfo,
}

/// `foundAffiliatePrograms`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FoundPrograms {
    pub total_count: i32,
    pub programs: Vec<FoundProgram>,
    pub next_offset: String,
}

impl FoundPrograms {
    pub fn parse(value: &Value) -> Self {
        Self {
            total_count: int(value.get("total_count")) as i32,
            programs: value
                .get("programs")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| {
                            Some(FoundProgram {
                                bot_user_id: int(row.get("bot_user_id")),
                                info: ProgramInfo::parse(row.get("info"))?,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            next_offset: text(value, "next_offset"),
        }
    }
}

/// `connectedAffiliateProgram`: a joined program and its referral link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedProgram {
    pub url: String,
    pub bot_user_id: i64,
    pub params: ProgramParams,
    pub connection_date: i32,
    pub is_disconnected: bool,
    pub user_count: i64,
    pub revenue_star_count: i64,
}

impl ConnectedProgram {
    pub fn parse(value: &Value) -> Self {
        Self {
            url: text(value, "url"),
            bot_user_id: int(value.get("bot_user_id")),
            params: ProgramParams::parse(value.get("parameters")),
            connection_date: int(value.get("connection_date")) as i32,
            is_disconnected: value
                .get("is_disconnected")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            user_count: int(value.get("user_count")),
            revenue_star_count: int(value.get("revenue_star_count")),
        }
    }
}

/// `connectedAffiliatePrograms`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectedPrograms {
    pub total_count: i32,
    pub programs: Vec<ConnectedProgram>,
    pub next_offset: String,
}

impl ConnectedPrograms {
    pub fn parse(value: &Value) -> Self {
        Self {
            total_count: int(value.get("total_count")) as i32,
            programs: value
                .get("programs")
                .and_then(Value::as_array)
                .map(|rows| rows.iter().map(ConnectedProgram::parse).collect())
                .unwrap_or_default(),
            next_offset: text(value, "next_offset"),
        }
    }
}

/// `AffiliateProgramSortOrder` and the "Sort by" menu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ProgramSort {
    #[default]
    Profitability,
    Revenue,
    Date,
}

impl ProgramSort {
    pub const ALL: [ProgramSort; 3] = [Self::Profitability, Self::Revenue, Self::Date];

    pub fn label(self) -> &'static str {
        match self {
            Self::Profitability => "Profitability",
            Self::Revenue => "Revenue",
            Self::Date => "Date",
        }
    }

    pub fn type_name(self) -> &'static str {
        match self {
            Self::Profitability => "affiliateProgramSortOrderProfitability",
            Self::Revenue => "affiliateProgramSortOrderRevenue",
            Self::Date => "affiliateProgramSortOrderCreationDate",
        }
    }
}

/// The commission stops of the setup slider (tdesktop
/// `setupCommission`): every whole percent between the TDLib limits, plus
/// the current value when it falls between them.
pub fn commission_stops(min_permille: i32, max_permille: i32, current: i32) -> Vec<i32> {
    let min = min_permille.clamp(1, 998);
    let max = max_permille.clamp(min + 1, 999);
    let current = current.clamp(min, max);
    let (low, high) = ((min + 9) / 10, max / 10);
    let mut values = Vec::new();
    if current < low * 10 {
        values.push(current);
    }
    for i in low..=high {
        values.push(i * 10);
        if i * 10 < current && (i == high || (i + 1) * 10 > current) {
            values.push(current);
        }
    }
    values
}

/// The duration stops of the setup slider: tdesktop's list plus the
/// current value when it is not on it, sorted. `months` 0 is forever.
pub fn duration_stops(months: i32) -> Vec<i32> {
    let current = if months == 0 {
        super::DURATION_FOREVER
    } else {
        months
    };
    let mut values = DURATION_STOPS.to_vec();
    if !values.contains(&current) {
        values.push(current);
        values.sort_unstable();
    }
    values
}

/// The setup page's starting values (tdesktop `ValueForCommission`,
/// `ValueForDurationMonths`): a running program's, else the defaults.
pub fn setup_start(program: Option<ProgramInfo>) -> ProgramParams {
    let program = program.unwrap_or_default();
    let exists = program.is_running();
    ProgramParams {
        commission_per_mille: if program.params.commission_per_mille > 0 {
            program.params.commission_per_mille
        } else {
            COMMISSION_DEFAULT
        },
        month_count: if program.params.month_count > 0 {
            program.params.month_count
        } else if exists {
            0
        } else {
            DURATION_DEFAULT
        },
    }
}
