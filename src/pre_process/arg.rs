use std::fmt::Display;
use std::str::FromStr;

use crate::functions::table::Table;
use crate::parser::{self, RawArgGroup, RawClause};
use crate::pre_process::block::{COMMANDS, DATA_NAMES, FUNCTIONS};
use crate::{functions::display::RenderContext, TimeFrequency};
use anyhow::{anyhow, Error, Result};
use chrono::NaiveDate;

use super::block::Expression;
use super::flatten::{find_table, mutate, ItemOrCollection};
use super::tree::{Component, Node, Text};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arg {
    Command(&'static str),
    Function(&'static str),
    TimeFrequency(TimeFrequency),
    DataName(&'static str),
    Date(NaiveDate),
    RenderContext(RenderContext),
}

impl Arg {
    /// Matches a name keyword to an enum variant and tries to create that variant from `s`
    pub fn from_labelled_str(name: &str, s: &str) -> Result<Self> {
        let arg = match name {
            "command" => Arg::try_new_command(s),
            "function" => Arg::try_new_function(s),
            "frequency" => Arg::try_new_time_frequency(s),
            "name" => Arg::try_new_data_name(s),
            "date" => Arg::try_new_date(s),
            "display" => Arg::try_new_render_context(s),
            _ => return Err(anyhow!("Unknown arg type {name}")),
        };
        arg.ok_or_else(|| anyhow!("Named arg of type {name} is unknown: {s}"))
    }

    fn try_new_command(s: &str) -> Option<Self> {
        COMMANDS
            .keys()
            .find(|command| **command == s)
            .and_then(|command| Some(Arg::Command(command)))
    }
    fn try_new_function(s: &str) -> Option<Self> {
        FUNCTIONS
            .get(s)
            .and_then(|function| Some(Arg::Function(function)))
    }
    fn try_new_time_frequency(s: &str) -> Option<Self> {
        s.parse::<TimeFrequency>()
            .and_then(|frequency| Ok(Arg::TimeFrequency(frequency)))
            .ok()
    }
    fn try_new_data_name(s: &str) -> Option<Self> {
        DATA_NAMES
            .keys()
            .find(|name| *name == &s.to_string())
            .and_then(|name| Some(Arg::DataName(name.as_str())))
    }
    fn try_new_date(s: &str) -> Option<Self> {
        parser::parse_date(s)
            .and_then(|date| Ok(Arg::Date(date)))
            .ok()
    }
    fn try_new_render_context(s: &str) -> Option<Self> {
        s.parse::<RenderContext>()
            .and_then(|render_context| Ok(Arg::RenderContext(render_context)))
            .ok()
    }
}

impl FromStr for Arg {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for f in [
            Arg::try_new_command,
            Arg::try_new_function,
            Arg::try_new_time_frequency,
            Arg::try_new_data_name,
            Arg::try_new_date,
            Arg::try_new_render_context,
        ] {
            if let Some(arg) = f(s) {
                return Ok(arg);
            }
        }
        Err(anyhow!("Unnamed arg is unknown: {s}"))
    }
}

impl Display for Arg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(text) = match self {
            Arg::Command(command) => COMMANDS
                .get(command)
                .and_then(|command| Some(command.to_string())),
            Arg::Function(function) => Some(function.to_string()),
            Arg::TimeFrequency(frequency) => Some(frequency.to_string()),
            Arg::DataName(name) => DATA_NAMES
                .get(&name.to_string())
                .and_then(|name| Some(name.to_owned())),
            Arg::Date(date) => Some(date.format("%Y-%m-%d").to_string()),
            Arg::RenderContext(rc) => Some(rc.to_string()),
        } {
            write!(f, "{}", text)
        } else {
            panic!("Failed to Display Arg");
        }
    }
}

pub enum ArgGroup<'a> {
    Arg(Arg),
    Collection(Vec<Arg>),
    NamedCollection(&'a str, Vec<Arg>),
}

impl<'a> TryFrom<RawArgGroup<'a>> for ArgGroup<'a> {
    type Error = Error;

    fn try_from(raw: RawArgGroup<'a>) -> Result<Self, Self::Error> {
        match raw {
            RawArgGroup::Arg(arg) => Ok(ArgGroup::Arg(arg.parse::<Arg>()?)),
            RawArgGroup::LabelledArg(label, arg) => {
                Ok(ArgGroup::Arg(Arg::from_labelled_str(label, arg)?))
            }
            RawArgGroup::Collection(args) => Ok(ArgGroup::Collection(
                args.into_iter()
                    .map(|arg| arg.parse::<Arg>())
                    .collect::<Result<Vec<_>>>()?,
            )),
            RawArgGroup::NamedCollection(name, args) => Ok(ArgGroup::NamedCollection(
                name,
                args.into_iter()
                    .map(|arg| arg.parse::<Arg>())
                    .collect::<Result<Vec<_>>>()?,
            )),
        }
    }
}

impl ItemOrCollection for ArgGroup<'_> {
    type Item = Arg;

    fn items(&self) -> &[Self::Item] {
        match self {
            ArgGroup::Collection(collection) | ArgGroup::NamedCollection(_, collection) => {
                collection
            }
            ArgGroup::Arg(arg) => std::slice::from_ref(arg),
        }
    }
}

#[derive(Debug)]
pub enum Function {
    Expressions(Vec<Expression>),
    Table(Table),
}

impl Function {
    /// Matches a name keyword to an enum variant and tries to create that variant from `groups`.
    pub fn from_labelled_groups(name: &str, groups: Vec<ArgGroup<'_>>) -> Result<Self> {
        match name {
            "expression" => Function::try_new_expression(groups),
            "table" => Function::try_new_table(groups),
            _ => return Err(anyhow!("Unknown function type {name}")),
        }
    }

    /// Assumes an unlabelled group is an `Expression`, so tries to create that variant.
    pub fn from_groups(groups: Vec<ArgGroup<'_>>) -> Result<Self> {
        Function::try_new_expression(groups)
    }

    fn try_new_expression(groups: Vec<ArgGroup<'_>>) -> Result<Self> {
        let var_groups = mutate(ItemOrCollection::collection(groups));

        // Translate to expressions by filling args by order into an empty expression
        let expressions = var_groups
            .into_iter()
            .map(|group| {
                group.into_iter().fold(Expression::new(), |mut expr, arg| {
                    expr.fill_arg(arg);
                    expr
                })
            })
            .collect::<Vec<Expression>>();

        Ok(Function::Expressions(expressions))
    }
    fn try_new_table(groups: Vec<ArgGroup<'_>>) -> Result<Self> {
        Ok(Function::Table(find_table(groups)?))
    }
}

impl Component for Function {
    fn render(&self, ctx: &Expression) -> Result<String> {
        match self {
            Function::Expressions(exprs) => Ok(exprs
                .into_iter()
                .map(|expr| expr.render(ctx))
                .collect::<Result<Vec<String>>>()?
                .join(" ")),
            Function::Table(t) => t.render(ctx),
        }
    }
}

pub enum Clause<'a> {
    Function(Vec<ArgGroup<'a>>),
    NamedFunction(&'a str, Vec<ArgGroup<'a>>),
    Block(Vec<ArgGroup<'a>>, Vec<Clause<'a>>),
    Text(&'a str),
}

impl Clause<'_> {
    pub fn to_component(self) -> Result<Box<dyn Component>> {
        match self {
            Clause::Function(groups) => Ok(Box::new(Function::from_groups(groups)?)),
            Clause::NamedFunction(name, groups) => {
                Ok(Box::new(Function::from_labelled_groups(name, groups)?))
            }
            Clause::Block(groups, clauses) => {
                let exprs = mutate(ItemOrCollection::collection(groups))
                    .into_iter()
                    .map(|group| {
                        group.into_iter().fold(Expression::new(), |mut expr, arg| {
                            expr.fill_arg(arg);
                            expr
                        })
                    })
                    .collect::<Vec<Expression>>();

                let base_node = clauses.into_iter().try_fold(
                    Node::new(Expression::new()),
                    |mut node, clause| -> Result<Node> {
                        node.add_child(clause.to_component()?);
                        Ok(node)
                    },
                )?;

                let node = exprs.into_iter().fold(
                    Node::new(Expression::new()),
                    |mut parent, child_expr| {
                        parent.add_child(Box::new(base_node.with_expr(child_expr)));
                        parent
                    },
                );

                Ok(Box::new(node))
            }
            Clause::Text(text) => Ok(Box::new(Text(text.to_string()))),
        }
    }
}

impl<'a> TryFrom<RawClause<'a>> for Clause<'a> {
    type Error = Error;

    fn try_from(raw: RawClause<'a>) -> Result<Self, Self::Error> {
        match raw {
            RawClause::Function(arg_groups) => Ok(Clause::Function(
                arg_groups
                    .into_iter()
                    .map(|group| ArgGroup::try_from(group))
                    .collect::<Result<_>>()?,
            )),
            RawClause::NamedFunction(name, arg_groups) => Ok(Clause::NamedFunction(
                name,
                arg_groups
                    .into_iter()
                    .map(|group| ArgGroup::try_from(group))
                    .collect::<Result<_>>()?,
            )),
            RawClause::Block(arg_groups, clauses) => Ok(Clause::Block(
                arg_groups
                    .into_iter()
                    .map(|group| ArgGroup::try_from(group))
                    .collect::<Result<_>>()?,
                clauses
                    .into_iter()
                    .map(|clause| Clause::try_from(clause))
                    .collect::<Result<_>>()?,
            )),
            RawClause::Text(text) => Ok(Clause::Text(text)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arg() {
        assert_eq!("fig".parse::<Arg>().unwrap(), Arg::Command("fig"));
    }
}
