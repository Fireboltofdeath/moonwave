use std::convert::TryFrom;

use crate::{
    diagnostic::{Diagnostic, Diagnostics},
    doc_comment::DocComment,
    span::Span,
    tags::{KindTag, KindTagType, Tag, WithinTag},
};
use full_moon::ast::Stmt;

mod class;
mod function;
mod property;

pub use class::ClassDocEntry;
pub use function::{FunctionDocEntry, FunctionType};
pub use property::PropertyDocEntry;

/// Enum used when determining the type of the DocEntry during parsing
#[derive(Debug, PartialEq)]
enum DocEntryKind {
    Function {
        within: String,
        name: String,
        function_type: FunctionType,
    },
    Property {
        within: String,
        name: String,
    },
    Class {
        name: String,
    },
}

/// An enum of all possible DocEntries
#[derive(Debug)]
pub enum DocEntry<'a> {
    Function(FunctionDocEntry<'a>),
    Property(PropertyDocEntry<'a>),
    Class(ClassDocEntry<'a>),
}

#[derive(Debug)]
struct DocEntryParseArguments<'a> {
    name: String,
    tags: Vec<Tag<'a>>,
    desc: String,
    within: Option<String>,
}

// TODO: Within tags > 1
// TODO: Kind tags > 1
// TODO: Within tag incompatible with class tag
// TODO: Within tag required for kind tags other than class

fn get_explicit_kind(tags: &[Tag]) -> Result<Option<DocEntryKind>, Diagnostic> {
    let kind_tags = tags
        .iter()
        .filter(|t| matches!(**t, Tag::Kind(_)))
        .collect::<Vec<_>>();

    let within_tags = tags
        .iter()
        .filter(|t| matches!(**t, Tag::Within(_)))
        .collect::<Vec<_>>();

    if kind_tags.is_empty() {
        return Ok(None);
    } else if kind_tags.len() > 1 {
        return Err(kind_tags[1].diagnostic("Only one kind tag is allowed"));
    }

    let the_kind_tag = kind_tags[0];

    match the_kind_tag {
        Tag::Kind(KindTag {
            tag_type: KindTagType::Class,
            name,
            ..
        }) => {
            // TODO: Within tag incompatible with class tag

            Ok(Some(DocEntryKind::Class {
                name: name.as_str().to_owned(),
            }))
        }
        Tag::Kind(KindTag { tag_type, name, .. }) => {
            if within_tags.is_empty() {
                return Err(
                    the_kind_tag.diagnostic("Must specify containing class with @within tag")
                );
            }

            let name = name.as_str().to_owned();
            let within = match within_tags[0] {
                Tag::Within(WithinTag { name, .. }) => name.as_str().to_owned(),
                _ => unreachable!(),
            };

            match tag_type {
                KindTagType::Function => Ok(Some(DocEntryKind::Function {
                    name,
                    within,
                    function_type: FunctionType::Function,
                })),
                KindTagType::Property => Ok(Some(DocEntryKind::Property { name, within })),
                _ => panic!("Unhandled tag type {:?}", tag_type),
            }
        }
        _ => unreachable!(),
    }
}

fn determine_kind(
    doc_comment: &DocComment,
    stmt: &Stmt,
    tags: &[Tag],
) -> Result<DocEntryKind, Diagnostic> {
    let explicit_kind = get_explicit_kind(tags)?;

    if let Some(kind) = explicit_kind {
        return Ok(kind);
    }

    match stmt {
        Stmt::FunctionDeclaration(function) => match function.name().method_name() {
            Some(method_name) => Ok(DocEntryKind::Function {
                name: method_name.to_string(),
                within: function.name().names().to_string(),
                function_type: FunctionType::Method,
            }),
            None => {
                let mut names: Vec<_> = function.name().names().iter().collect();

                let function_name = names.pop().unwrap().to_string();

                if names.is_empty() {
                    return Err(Diagnostic::from_doc_comment(
                        "Function requires @within tag",
                        doc_comment,
                    ));
                }

                Ok(DocEntryKind::Function {
                    name: function_name,
                    within: names
                        .into_iter()
                        .map(|token| token.to_string())
                        .collect::<Vec<_>>()
                        .join("."),
                    function_type: FunctionType::Function,
                })
            }
        },

        Stmt::LocalAssignment(_) => unimplemented!(),
        Stmt::LocalFunction(_) => unimplemented!(),
        _ => Err(Diagnostic::from_doc_comment(
            "Explicitly specify a kind tag, like @function, @property, or @class.",
            doc_comment,
        )),
    }
}

impl<'a> DocEntry<'a> {
    pub fn parse(
        doc_comment: &'a DocComment,
        stmt: &Stmt<'a>,
    ) -> Result<DocEntry<'a>, Diagnostics> {
        let span: Span<'a> = doc_comment.into();

        let (tag_lines, desc_lines): (Vec<Span>, Vec<Span>) = span
            .lines()
            .map(Span::trim)
            .filter(|line| !line.is_empty())
            .partition(|line| line.starts_with('@'));

        let desc = desc_lines
            .iter()
            .map(|span| span.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let (tags, errors): (Vec<_>, Vec<_>) = tag_lines
            .into_iter()
            .map(Tag::try_from)
            .partition(Result::is_ok);

        let mut tags: Vec<_> = tags.into_iter().map(Result::unwrap).collect();
        let errors: Vec<_> = errors.into_iter().map(Result::unwrap_err).collect();

        if !errors.is_empty() {
            return Err(Diagnostics::from(errors));
        }

        let kind =
            determine_kind(doc_comment, stmt, &tags).map_err(|err| Diagnostics::from(vec![err]))?;

        // Sift out the kind/within tags because those are only used for determining the kind
        tags.retain(|t| !matches!(t, Tag::Kind(_) | Tag::Within(_)));

        Ok(match kind {
            DocEntryKind::Function {
                within,
                name,
                function_type,
            } => DocEntry::Function(FunctionDocEntry::parse(
                DocEntryParseArguments {
                    within: Some(within),
                    name,
                    desc,
                    tags,
                },
                function_type,
            )?),
            DocEntryKind::Property { within, name } => {
                DocEntry::Property(PropertyDocEntry::parse(DocEntryParseArguments {
                    within: Some(within),
                    name,
                    desc,
                    tags,
                })?)
            }
            DocEntryKind::Class { name } => {
                DocEntry::Class(ClassDocEntry::parse(DocEntryParseArguments {
                    within: None,
                    name,
                    desc,
                    tags,
                })?)
            }
        })
    }
}
