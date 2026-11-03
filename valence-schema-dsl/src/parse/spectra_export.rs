//! `spectra_export: { cadence, job_name?, target_table? }` grammar.
//!
//! Presence of the block opts the schema in. Schema authors pick a [`ParsedSpectraExportCadence`]
//! variant; cron strings are platform-owned and never appear in the DSL.

use syn::punctuated::Punctuated;
use syn::{
    braced,
    parse::{Parse, ParseStream},
    token, Ident, Lit, LitStr, Result, Token,
};

/// Fixed export cadence accepted by `spectra_export: { cadence: ... }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedSpectraExportCadence {
    Hourly,
    ThreeHours,
    SixHours,
    TwelveHours,
    Daily,
    Weekly,
}

impl ParsedSpectraExportCadence {
    const ALL: [Self; 6] = [
        Self::Hourly,
        Self::ThreeHours,
        Self::SixHours,
        Self::TwelveHours,
        Self::Daily,
        Self::Weekly,
    ];

    /// Variant name as written in the DSL (`ThreeHours`).
    #[must_use]
    pub fn variant_name(self) -> &'static str {
        match self {
            Self::Hourly => "Hourly",
            Self::ThreeHours => "ThreeHours",
            Self::SixHours => "SixHours",
            Self::TwelveHours => "TwelveHours",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
        }
    }

    /// Lower snake form used in the default job name (`three_hours`).
    #[must_use]
    pub fn snake_name(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::ThreeHours => "three_hours",
            Self::SixHours => "six_hours",
            Self::TwelveHours => "twelve_hours",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    fn from_ident(ident: &Ident) -> Result<Self> {
        let name = ident.to_string();
        Self::ALL
            .into_iter()
            .find(|c| c.variant_name() == name)
            .ok_or_else(|| {
                syn::Error::new(
                    ident.span(),
                    format!(
                        "Unknown spectra_export cadence `{name}`; expected one of {}",
                        Self::expected_list()
                    ),
                )
            })
    }

    fn expected_list() -> String {
        Self::ALL
            .iter()
            .map(|c| c.variant_name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Parsed `spectra_export` block with defaults already resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSpectraExportConfig {
    pub cadence: ParsedSpectraExportCadence,
    /// Explicit `job_name`, else `{table}_{cadence}_export`.
    pub job_name: String,
    /// Explicit `target_table`, else `{table}_export`.
    pub target_table: String,
}

/// `spectra_export: { cadence: Daily, job_name: "...", target_table: "..." }`
pub struct SpectraExportConfig {
    _brace: token::Brace,
    pub items: Punctuated<SpectraExportItem, Token![,]>,
}

impl Parse for SpectraExportConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(SpectraExportConfig {
            _brace: braced!(content in input),
            items: content.parse_terminated(SpectraExportItem::parse, Token![,])?,
        })
    }
}

pub enum SpectraExportItem {
    Cadence(ParsedSpectraExportCadence),
    JobName(LitStr),
    TargetTable(LitStr),
}

impl Parse for SpectraExportItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        match key.to_string().as_str() {
            "cadence" => {
                if input.peek(Lit) {
                    return Err(syn::Error::new(
                        input.span(),
                        format!(
                            "spectra_export.cadence takes a platform cadence, not a cron string; \
                             expected one of {}",
                            ParsedSpectraExportCadence::expected_list()
                        ),
                    ));
                }
                let ident: Ident = input.parse()?;
                Ok(SpectraExportItem::Cadence(
                    ParsedSpectraExportCadence::from_ident(&ident)?,
                ))
            }
            "job_name" => Ok(SpectraExportItem::JobName(input.parse()?)),
            "target_table" => Ok(SpectraExportItem::TargetTable(input.parse()?)),
            _ => Err(syn::Error::new(
                key.span(),
                format!("Unknown spectra_export key: {key}"),
            )),
        }
    }
}

fn checked_name(lit: &LitStr, key: &str) -> Result<String> {
    let value = lit.value();
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(value)
    } else {
        Err(syn::Error::new(
            lit.span(),
            format!(
                "spectra_export.{key} must be non-empty and use only ASCII letters, digits, `_` or `-`"
            ),
        ))
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, key: &str, span: proc_macro2::Span) -> Result<()> {
    if slot.replace(value).is_some() {
        return Err(syn::Error::new(
            span,
            format!("Duplicate spectra_export key: {key}"),
        ));
    }
    Ok(())
}

pub(super) fn parse_spectra_export_config(
    config: &SpectraExportConfig,
    table: &str,
) -> Result<ParsedSpectraExportConfig> {
    let span = proc_macro2::Span::call_site();
    let mut cadence = None;
    let mut job_name = None;
    let mut target_table = None;
    for item in &config.items {
        match item {
            SpectraExportItem::Cadence(c) => set_once(&mut cadence, *c, "cadence", span)?,
            SpectraExportItem::JobName(v) => {
                set_once(
                    &mut job_name,
                    checked_name(v, "job_name")?,
                    "job_name",
                    v.span(),
                )?;
            }
            SpectraExportItem::TargetTable(v) => set_once(
                &mut target_table,
                checked_name(v, "target_table")?,
                "target_table",
                v.span(),
            )?,
        }
    }

    let cadence = cadence.ok_or_else(|| {
        syn::Error::new(
            span,
            format!(
                "spectra_export.cadence is required; expected one of {}",
                ParsedSpectraExportCadence::expected_list()
            ),
        )
    })?;

    Ok(ParsedSpectraExportConfig {
        cadence,
        job_name: job_name.unwrap_or_else(|| format!("{table}_{}_export", cadence.snake_name())),
        target_table: target_table.unwrap_or_else(|| format!("{table}_export")),
    })
}
