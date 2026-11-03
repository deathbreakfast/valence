use syn::punctuated::Punctuated;
use syn::{
    braced, bracketed,
    parse::{Parse, ParseStream},
    token, Ident, LitBool, LitStr, Result, Token,
};

#[derive(Debug, Clone)]
pub struct ParsedConnection {
    pub name: String,
    pub table: String,
    pub cardinality: String,
    pub required: bool,
    pub on_delete: String,
    pub model: Option<String>,
    pub reverse_field: Option<String>,
    pub edge_table: Option<String>,
    /// Built-in privacy rule for edge writes (`SourceUpdateTargetRead`); ManyToMany only.
    pub edge_policy: Option<String>,
    pub target_trait: Option<String>,
}

/// Connections block: `connections: [ name: { table, model?, ... }, ... ]`
#[derive(Debug, Clone)]
pub struct ConnectionsConfig {
    _bracket: token::Bracket,
    pub connections: Punctuated<ConnectionSpec, Token![,]>,
}

#[derive(Debug, Clone)]
pub struct ConnectionSpec {
    pub name: Ident,
    _colon: Token![:],
    _brace: token::Brace,
    pub attrs: Punctuated<ConnectionAttr, Token![,]>,
}

#[derive(Debug, Clone)]
pub enum ConnectionAttr {
    Table(LitStr),
    Cardinality(Ident),
    Required(LitBool),
    OnDelete(Ident),
    Model(LitStr),
    ReverseField(LitStr),
    EdgeTable(LitStr),
    EdgePolicy(Ident),
    TargetTrait(LitStr),
}

impl Parse for ConnectionsConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(ConnectionsConfig {
            _bracket: bracketed!(content in input),
            connections: content.parse_terminated(ConnectionSpec::parse, Token![,])?,
        })
    }
}

impl Parse for ConnectionSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        let colon: Token![:] = input.parse()?;
        let content;
        let brace = braced!(content in input);
        let attrs = content.parse_terminated(ConnectionAttr::parse, Token![,])?;
        Ok(ConnectionSpec {
            name,
            _colon: colon,
            _brace: brace,
            attrs,
        })
    }
}

impl Parse for ConnectionAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        match key.to_string().as_str() {
            "table" => Ok(ConnectionAttr::Table(input.parse()?)),
            "cardinality" => Ok(ConnectionAttr::Cardinality(input.parse()?)),
            "required" => Ok(ConnectionAttr::Required(input.parse()?)),
            "on_delete" => Ok(ConnectionAttr::OnDelete(input.parse()?)),
            "model" | "target" => Ok(ConnectionAttr::Model(input.parse()?)),
            "reverse_field" => Ok(ConnectionAttr::ReverseField(input.parse()?)),
            "edge_table" => Ok(ConnectionAttr::EdgeTable(input.parse()?)),
            "edge_policy" => Ok(ConnectionAttr::EdgePolicy(input.parse()?)),
            "target_trait" => Ok(ConnectionAttr::TargetTrait(input.parse()?)),
            _ => Err(syn::Error::new(
                key.span(),
                format!("Unknown connection key: {key}"),
            )),
        }
    }
}

/// Lower a connections block into [`ParsedConnection`] values.
pub fn parse_connections(config: &ConnectionsConfig) -> Result<Vec<ParsedConnection>> {
    let mut out = Vec::new();

    for conn in &config.connections {
        let mut table: Option<String> = None;
        let mut cardinality = "HasOne".to_string();
        let mut required = true;
        let mut on_delete: Option<String> = None;
        let mut model = None;
        let mut reverse_field = None;
        let mut edge_table = None;
        let mut edge_policy: Option<Ident> = None;
        let mut target_trait = None;

        for attr in &conn.attrs {
            match attr {
                ConnectionAttr::Table(v) => table = Some(v.value()),
                ConnectionAttr::Cardinality(v) => cardinality = v.to_string(),
                ConnectionAttr::Required(v) => required = v.value,
                ConnectionAttr::OnDelete(v) => on_delete = Some(v.to_string()),
                ConnectionAttr::Model(v) => model = Some(v.value()),
                ConnectionAttr::ReverseField(v) => reverse_field = Some(v.value()),
                ConnectionAttr::EdgeTable(v) => edge_table = Some(v.value()),
                ConnectionAttr::EdgePolicy(v) => edge_policy = Some(v.clone()),
                ConnectionAttr::TargetTrait(v) => target_trait = Some(v.value()),
            }
        }

        let table = table
            .or_else(|| target_trait.as_ref().map(|t| format!("trait:{t}")))
            .ok_or_else(|| {
                syn::Error::new(
                    conn.name.span(),
                    format!(
                        "Connection '{}' missing table: (or target_trait:)",
                        conn.name
                    ),
                )
            })?;

        let on_delete = on_delete.ok_or_else(|| {
            syn::Error::new(
                conn.name.span(),
                format!(
                    "Connection '{}' is missing required `on_delete:` (Cascade | SetNull | Restrict)",
                    conn.name
                ),
            )
        })?;

        if let Some(policy) = &edge_policy {
            if policy != "SourceUpdateTargetRead" {
                return Err(syn::Error::new(
                    policy.span(),
                    format!(
                        "Connection '{}' has unknown edge_policy `{policy}` (expected SourceUpdateTargetRead)",
                        conn.name
                    ),
                ));
            }
            if cardinality != "ManyToMany" {
                return Err(syn::Error::new(
                    policy.span(),
                    format!(
                        "Connection '{}' sets edge_policy, which only applies to cardinality ManyToMany (found {cardinality})",
                        conn.name
                    ),
                ));
            }
        }

        out.push(ParsedConnection {
            name: conn.name.to_string(),
            table,
            cardinality,
            required,
            on_delete,
            model,
            reverse_field,
            edge_table,
            edge_policy: edge_policy.map(|p| p.to_string()),
            target_trait,
        });
    }

    Ok(out)
}
