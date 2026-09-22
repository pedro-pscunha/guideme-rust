//! `#[derive(Choice)]` and `#[derive(Levels)]` for `guideme`.
//!
//! Both work on enums whose variants are all unit variants. A variant's `///` doc comment is
//! its rubric; `#[guide(rubric = "…")]` overrides it. `Choice` keys default to the variant
//! name in `snake_case`; `#[guide(key = "…")]` overrides. At most one `#[guide(fallback)]`.
//!
//! `#[guide(example = "…")]` and `#[guide(counterexample = "…")]` are repeatable and compose
//! into the rubric the model reads: an `Examples:` line and a `Not this option:` line, items
//! joined with `; `. A variant with neither renders to its rubric unchanged, byte for byte.
//! `counterexample` is a `Choice` key only: a level is a position on a scale, not an option to
//! rule out.

use heck::ToSnakeCase;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Ident, LitStr, Meta, Result, parse_macro_input};

/// One enum variant with its wire key, rubric, examples and fallback flag resolved.
struct Variant {
    ident: Ident,
    key: String,
    rubric: Option<String>,
    examples: Vec<String>,
    counterexamples: Vec<String>,
    fallback: bool,
}

impl Variant {
    /// The rubric as the model reads it, or `None` when the variant has none.
    fn rendered(&self) -> Option<String> {
        self.rubric
            .as_ref()
            .map(|what| render(what, &self.examples, &self.counterexamples))
    }
}

/// Compose a rubric: the description, then the example clauses, one per line.
///
/// With no examples and no counterexamples the output is `what` itself, byte for byte — a
/// rubric written before this existed puts the same bytes on the wire. `what` is used verbatim:
/// never trimmed, never re-punctuated. This algorithm is a cross-SDK contract item; see
/// `docs/contract.md`.
fn render(what: &str, examples: &[String], counterexamples: &[String]) -> String {
    if examples.is_empty() && counterexamples.is_empty() {
        return what.to_owned();
    }
    let mut out = what.to_owned();
    if !examples.is_empty() {
        out.push_str("\nExamples: ");
        out.push_str(&examples.join("; "));
    }
    if !counterexamples.is_empty() {
        out.push_str("\nNot this option: ");
        out.push_str(&counterexamples.join("; "));
    }
    out
}

/// Parse one repeatable `#[guide(example = "…")]` / `#[guide(counterexample = "…")]` value.
fn push_item(
    meta: &syn::meta::ParseNestedMeta<'_>,
    key: &str,
    into: &mut Vec<String>,
) -> Result<()> {
    let lit = meta.value()?.parse::<LitStr>()?;
    let value = lit.value();
    if value.trim().is_empty() {
        return Err(Error::new_spanned(
            &lit,
            format!("guideme: #[guide({key})] must not be empty"),
        ));
    }
    if into.contains(&value) {
        return Err(Error::new_spanned(
            &lit,
            format!("guideme: duplicate #[guide({key} = {value:?})] on this variant"),
        ));
    }
    into.push(value);
    Ok(())
}

/// Derive `guideme::Options` for a unit-variant enum.
#[proc_macro_derive(Choice, attributes(guide))]
pub fn derive_choice(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_choice(&input)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

/// Derive `guideme::Levels` for a unit-variant enum.
#[proc_macro_derive(Levels, attributes(guide))]
pub fn derive_levels(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_levels(&input)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

/// Read the enum's variants, applying `#[guide(..)]` overrides to key, rubric and fallback.
fn variants(input: &DeriveInput, derive: &str) -> Result<Vec<Variant>> {
    let Data::Enum(data) = &input.data else {
        return Err(Error::new(
            Span::call_site(),
            format!("guideme: #[derive({derive})] only supports enums"),
        ));
    };
    let mut out = Vec::with_capacity(data.variants.len());
    for v in &data.variants {
        if !matches!(v.fields, Fields::Unit) {
            return Err(Error::new_spanned(
                v,
                format!(
                    "guideme: #[derive({derive})] needs unit variants; `{}` has fields",
                    v.ident
                ),
            ));
        }
        let mut key = v.ident.to_string().to_snake_case();
        let mut rubric = doc_comment(&v.attrs);
        let mut examples = Vec::new();
        let mut counterexamples = Vec::new();
        let mut fallback = false;
        for attr in v.attrs.iter().filter(|a| a.path().is_ident("guide")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("key") {
                    if derive == "Levels" {
                        return Err(meta.error(
                            "guideme: #[guide(key)] is not allowed on Levels; levels are positional",
                        ));
                    }
                    key = meta.value()?.parse::<LitStr>()?.value();
                } else if meta.path.is_ident("rubric") {
                    rubric = Some(meta.value()?.parse::<LitStr>()?.value());
                } else if meta.path.is_ident("fallback") {
                    fallback = true;
                } else if meta.path.is_ident("example") {
                    push_item(&meta, "example", &mut examples)?;
                } else if meta.path.is_ident("counterexample") {
                    if derive == "Levels" {
                        return Err(meta.error(
                            "guideme: #[guide(counterexample)] is not allowed on Levels; a level is a position on a scale, not an option to rule out",
                        ));
                    }
                    push_item(&meta, "counterexample", &mut counterexamples)?;
                } else {
                    return Err(meta.error(
                        "guideme: unknown #[guide(..)] option; expected key, rubric, fallback, example, or counterexample",
                    ));
                }
                Ok(())
            })?;
        }
        // Examples describe a rubric, so there has to be one to describe. A variant carrying
        // neither is left exactly as 0.1.0 read it, blank rubric included: rejecting that would
        // be a new error for code that has nothing to do with this feature.
        if !(examples.is_empty() && counterexamples.is_empty())
            && rubric.as_deref().is_none_or(|what| what.trim().is_empty())
        {
            let option = if examples.is_empty() {
                "counterexample"
            } else {
                "example"
            };
            return Err(Error::new_spanned(
                v,
                format!(
                    "guideme: #[guide({option})] on `{}` needs a non-empty rubric to attach to (a /// doc comment or #[guide(rubric = \"…\")])",
                    v.ident
                ),
            ));
        }
        // Items are joined onto one line, so a line break in one renders as a clause boundary
        // the declaration never wrote. Checked here rather than in `push_item` so that a
        // variant breaking this and the blank-rubric rule reports the blank one, matching the
        // order `Rubric::render` produces: `push_item` runs while the attributes are still
        // being read and cannot see a `rubric` set by a later one.
        for (option, items) in [("example", &examples), ("counterexample", &counterexamples)] {
            if let Some(item) = items.iter().find(|item| item.contains(['\n', '\r'])) {
                return Err(Error::new_spanned(
                    v,
                    format!(
                        "guideme: #[guide({option})] may not contain a line break (U+000A or U+000D): {item:?}"
                    ),
                ));
            }
        }
        out.push(Variant {
            ident: v.ident.clone(),
            key,
            rubric,
            examples,
            counterexamples,
            fallback,
        });
    }
    if out.len() < 2 {
        return Err(Error::new_spanned(
            &input.ident,
            format!("guideme: #[derive({derive})] needs at least two variants"),
        ));
    }
    check_examples(&out, derive)?;
    Ok(out)
}

/// An example asserts that an input belongs here, so it cannot also say it does not, and it
/// cannot say so of two variants at once. The same string as an example of one variant and a
/// counterexample of another is the confusable-options pattern, and stays legal.
fn check_examples(vs: &[Variant], derive: &str) -> Result<()> {
    let noun = if derive == "Levels" {
        "level"
    } else {
        "option"
    };
    for (i, v) in vs.iter().enumerate() {
        for example in &v.examples {
            if v.counterexamples.contains(example) {
                return Err(Error::new_spanned(
                    &v.ident,
                    format!(
                        "guideme: {example:?} is both an example and a counterexample of `{}`; it cannot be in and out of the same {noun}",
                        v.ident
                    ),
                ));
            }
            for other in &vs[..i] {
                if other.examples.contains(example) {
                    return Err(Error::new_spanned(
                        &v.ident,
                        format!(
                            "guideme: {example:?} is an example of both `{}` and `{}`; an input belongs to one {noun}",
                            other.ident, v.ident
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Join a variant's `///` lines into one rubric string.
fn doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| {
            if let Meta::NameValue(nv) = &a.meta
                && let syn::Expr::Lit(expr) = &nv.value
                && let syn::Lit::Str(text) = &expr.lit
            {
                Some(text.value().trim().to_owned())
            } else {
                None
            }
        })
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

/// Build the `::guideme::Options` impl.
fn expand_choice(input: &DeriveInput) -> Result<proc_macro2::TokenStream> {
    let vs = variants(input, "Choice")?;
    if vs.len() > 255 {
        return Err(Error::new_spanned(
            &input.ident,
            "guideme: a Choice may have at most 255 options",
        ));
    }
    for (i, a) in vs.iter().enumerate() {
        if let Some(first) = vs[..i].iter().find(|b| b.key == a.key) {
            return Err(Error::new_spanned(
                &a.ident,
                format!(
                    "guideme: duplicate key {:?} (also on `{}`)",
                    a.key, first.ident
                ),
            ));
        }
    }
    let fallbacks: Vec<&Variant> = vs.iter().filter(|v| v.fallback).collect();
    if let Some(second) = fallbacks.get(1) {
        return Err(Error::new_spanned(
            &second.ident,
            "guideme: only one variant may be #[guide(fallback)]",
        ));
    }
    let name = &input.ident;
    let idents = vs.iter().map(|v| &v.ident);
    let keys = vs.iter().map(|v| &v.key);
    let rubrics = vs.iter().map(|v| {
        if let Some(r) = v.rendered() {
            quote!(Some(#r))
        } else {
            quote!(None)
        }
    });
    let fallback = if let Some(v) = fallbacks.first() {
        let ident = &v.ident;
        quote!(Some(Self::#ident))
    } else {
        quote!(None)
    };
    Ok(quote! {
        impl ::guideme::Options for #name {
            const RUBRIC: &'static [(&'static str, Option<&'static str>)] = &[#((#keys, #rubrics)),*];
            fn from_key(key: &str) -> Option<Self> {
                const VARIANTS: &[#name] = &[#(#name::#idents),*];
                Self::RUBRIC
                    .iter()
                    .position(|(k, _)| *k == key)
                    .and_then(|i| VARIANTS.get(i).cloned())
            }
            fn fallback() -> Option<Self> {
                #fallback
            }
        }
    })
}

/// Build the `::guideme::Levels` impl.
fn expand_levels(input: &DeriveInput) -> Result<proc_macro2::TokenStream> {
    let vs = variants(input, "Levels")?;
    if vs.len() > 10 {
        return Err(Error::new_spanned(
            &input.ident,
            "guideme: a Score may have at most 10 levels",
        ));
    }
    if let Some(v) = vs.iter().find(|v| v.fallback) {
        return Err(Error::new_spanned(
            &v.ident,
            "guideme: #[guide(fallback)] is not allowed on Levels; use `.or(level)` at the call site",
        ));
    }
    let mut levels = Vec::with_capacity(vs.len());
    for v in &vs {
        let Some(rubric) = v.rendered() else {
            return Err(Error::new_spanned(
                &v.ident,
                format!(
                    "guideme: level `{}` needs a rubric (a /// doc comment or #[guide(rubric = \"…\")])",
                    v.ident
                ),
            ));
        };
        levels.push(rubric);
    }
    let name = &input.ident;
    let idents: Vec<&Ident> = vs.iter().map(|v| &v.ident).collect();
    let indices = 0..vs.len();
    Ok(quote! {
        impl ::guideme::Levels for #name {
            const LEVELS: &'static [&'static str] = &[#(#levels),*];
            fn from_index(index: usize) -> Option<Self> {
                const VARIANTS: &[#name] = &[#(#name::#idents),*];
                VARIANTS.get(index).cloned()
            }
            fn index(&self) -> usize {
                match self {
                    #(#name::#idents => #indices),*
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        missing_docs
    )]

    use proptest::prelude::*;

    use super::render;

    proptest! {
        /// The load-bearing invariant, on the macro's copy of the renderer. `guideme` property-
        /// tests the same law over `Rubric`: two copies of ~10 lines, so the property is worth
        /// asserting on each rather than pinning one against the other on fixed rows.
        #[test]
        fn a_rubric_with_no_parts_renders_to_itself(what in "(?s).{0,64}") {
            prop_assert_eq!(render(&what, &[], &[]), what);
        }
    }
}
