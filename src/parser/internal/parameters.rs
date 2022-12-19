use crate::lexer::token::TokenKind;
use crate::parser::ast::arguments::Argument;
use crate::parser::ast::arguments::ArgumentList;
use crate::parser::ast::functions::ConstructorParameter;
use crate::parser::ast::functions::ConstructorParameterList;
use crate::parser::ast::functions::FunctionParameter;
use crate::parser::ast::functions::FunctionParameterList;
use crate::parser::error::ParseError;
use crate::parser::error::ParseResult;
use crate::parser::expressions;
use crate::parser::internal::attributes;
use crate::parser::internal::data_type;
use crate::parser::internal::identifiers;
use crate::parser::internal::modifiers;
use crate::parser::internal::utils;
use crate::parser::internal::variables;
use crate::parser::state::State;

pub fn function_parameter_list(state: &mut State) -> Result<FunctionParameterList, ParseError> {
    let comments = state.stream.comments();
    let left_parenthesis = utils::skip_left_parenthesis(state)?;
    let parameters = utils::comma_separated(
        state,
        &|state| {
            attributes::gather_attributes(state)?;

            let ty = data_type::optional_data_type(state)?;

            let mut current = state.stream.current();
            let ampersand = if current.kind == TokenKind::Ampersand {
                state.stream.next();
                current = state.stream.current();
                Some(current.span)
            } else {
                None
            };

            let ellipsis = if current.kind == TokenKind::Ellipsis {
                state.stream.next();

                Some(current.span)
            } else {
                None
            };

            // 2. Then expect a variable.
            let var = variables::simple_variable(state)?;

            let mut default = None;
            if state.stream.current().kind == TokenKind::Equals {
                state.stream.next();
                default = Some(expressions::create(state)?);
            }

            Ok(FunctionParameter {
                comments: state.stream.comments(),
                name: var,
                attributes: state.get_attributes(),
                data_type: ty,
                ellipsis,
                default,
                ampersand,
            })
        },
        TokenKind::RightParen,
    )?;

    let right_parenthesis = utils::skip_right_parenthesis(state)?;

    Ok(FunctionParameterList {
        comments,
        left_parenthesis,
        parameters,
        right_parenthesis,
    })
}

pub fn constructor_parameter_list(
    state: &mut State,
    class_name: &str,
) -> Result<ConstructorParameterList, ParseError> {
    let comments = state.stream.comments();

    let left_parenthesis = utils::skip_left_parenthesis(state)?;
    let parameters = utils::comma_separated::<ConstructorParameter>(
        state,
        &|state| {
            attributes::gather_attributes(state)?;

            let modifiers = modifiers::promoted_property_group(modifiers::collect(state)?)?;

            let ty = data_type::optional_data_type(state)?;

            let mut current = state.stream.current();
            let ampersand = if matches!(current.kind, TokenKind::Ampersand) {
                state.stream.next();

                current = state.stream.current();

                Some(current.span)
            } else {
                None
            };

            let ellipsis = if matches!(current.kind, TokenKind::Ellipsis) {
                state.stream.next();
                if !modifiers.is_empty() {
                    return Err(ParseError::VariadicPromotedProperty(current.span));
                }

                Some(current.span)
            } else {
                None
            };

            // 2. Then expect a variable.
            let var = variables::simple_variable(state)?;

            if !modifiers.is_empty() {
                match &ty {
                    Some(ty) => {
                        if ty.includes_callable() || ty.is_bottom() {
                            return Err(ParseError::ForbiddenTypeUsedInProperty(
                                state.named(class_name),
                                var.to_string(),
                                ty.clone(),
                                state.stream.current().span,
                            ));
                        }
                    }
                    None => {
                        if modifiers.has_readonly() {
                            return Err(ParseError::MissingTypeForReadonlyProperty(
                                state.named(class_name),
                                var.to_string(),
                                state.stream.current().span,
                            ));
                        }
                    }
                }
            }

            let mut default = None;
            if state.stream.current().kind == TokenKind::Equals {
                state.stream.next();
                default = Some(expressions::create(state)?);
            }

            Ok(ConstructorParameter {
                comments: state.stream.comments(),
                name: var,
                attributes: state.get_attributes(),
                data_type: ty,
                ellipsis,
                default,
                modifiers,
                ampersand,
            })
        },
        TokenKind::RightParen,
    )?;

    let right_parenthesis = utils::skip_right_parenthesis(state)?;

    Ok(ConstructorParameterList {
        comments,
        left_parenthesis,
        parameters,
        right_parenthesis,
    })
}

pub fn argument_list(state: &mut State) -> ParseResult<ArgumentList> {
    let comments = state.stream.comments();
    let start = utils::skip_left_parenthesis(state)?;

    let mut arguments = Vec::new();
    let mut has_used_named_arguments = false;

    while !state.stream.is_eof() && state.stream.current().kind != TokenKind::RightParen {
        let span = state.stream.current().span;
        let (named, argument) = argument(state)?;
        if named {
            has_used_named_arguments = true;
        } else if has_used_named_arguments {
            return Err(ParseError::CannotUsePositionalArgumentAfterNamedArgument(
                span,
            ));
        }

        arguments.push(argument);

        if state.stream.current().kind == TokenKind::Comma {
            state.stream.next();
        } else {
            break;
        }
    }

    let end = utils::skip_right_parenthesis(state)?;

    Ok(ArgumentList {
        comments,
        left_parenthesis: start,
        right_parenthesis: end,
        arguments,
    })
}

fn argument(state: &mut State) -> ParseResult<(bool, Argument)> {
    if identifiers::is_identifier_maybe_reserved(&state.stream.current().kind)
        && state.stream.peek().kind == TokenKind::Colon
    {
        let name = identifiers::identifier_maybe_reserved(state)?;
        let colon = utils::skip(state, TokenKind::Colon)?;
        let ellipsis = if state.stream.current().kind == TokenKind::Ellipsis {
            Some(utils::skip(state, TokenKind::Ellipsis)?)
        } else {
            None
        };
        let value = expressions::create(state)?;

        return Ok((
            true,
            Argument::Named {
                comments: state.stream.comments(),
                name,
                colon,
                ellipsis,
                value,
            },
        ));
    }

    let ellipsis = if state.stream.current().kind == TokenKind::Ellipsis {
        Some(utils::skip(state, TokenKind::Ellipsis)?)
    } else {
        None
    };

    let value = expressions::create(state)?;

    Ok((
        false,
        Argument::Positional {
            comments: state.stream.comments(),
            ellipsis,
            value,
        },
    ))
}
