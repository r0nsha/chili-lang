use super::*;
use crate::{ast::pattern::Pattern, workspace::ModuleId};

impl Parser {
    pub fn try_parse_any_binding(
        &mut self,
        attrs: Vec<ast::Attr>,
        visibility: ast::Visibility,
        is_top_level: bool,
    ) -> DiagnosticResult<Option<DiagnosticResult<ast::Binding>>> {
        if eat!(self, Let) {
            Ok(Some(self.parse_binding(attrs, visibility, false)))
        } else if eat!(self, Fn) {
            if is!(self, Ident(_)) {
                Ok(Some(self.parse_function_binding(attrs, visibility)))
            } else {
                if is_top_level {
                    Err(SyntaxError::expected(self.span(), "an identifier"))
                } else {
                    // This is considrent Ok, since this could be
                    // a function expression or a function type
                    self.revert(1);
                    Ok(None)
                }
            }
        } else if eat!(self, Static) {
            if eat!(self, Let) {
                Ok(Some(self.parse_binding(attrs, visibility, true)))
            } else {
                // This is considered Ok, since this could be
                // another kind of static expression
                self.revert(1);
                Ok(None)
            }
        } else if eat!(self, Extern) {
            Ok(Some(self.parse_extern_binding(attrs, visibility)))
        } else if eat!(self, Type) {
            Ok(Some(self.parse_type_binding(attrs, visibility)))
        } else {
            Ok(None)
        }
    }

    pub fn parse_binding(
        &mut self,
        attrs: Vec<ast::Attr>,
        visibility: ast::Visibility,
        is_static: bool,
    ) -> DiagnosticResult<ast::Binding> {
        let start_span = self.previous_span();

        let pattern = self.parse_pattern()?;

        let type_expr = if eat!(self, Colon) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        require!(self, Eq, "=")?;

        let value = match &pattern {
            Pattern::Name(pattern) => self.parse_named_expr(pattern.name)?,
            _ => self.parse_expr()?,
        };

        Ok(ast::Binding {
            module_id: Default::default(),
            attrs,
            visibility,
            kind: ast::BindingKind::Orphan {
                pattern,
                type_expr,
                value: Box::new(value),
                is_static,
            },
            span: start_span.to(self.previous_span()),
        })
    }

    pub fn parse_function_binding(
        &mut self,
        attrs: Vec<ast::Attr>,
        visibility: ast::Visibility,
    ) -> DiagnosticResult<ast::Binding> {
        let start_span = self.previous_span();

        let id = require!(self, Ident(_), "an identifier")?;
        let name = id.name();

        let name_and_span = ast::NameAndSpan { name, span: id.span };

        let sig = self.parse_function_sig(Some(name), false)?;

        require!(self, Eq, "=")?;

        let body = Box::new(self.parse_expr()?);

        Ok(ast::Binding {
            module_id: Default::default(),
            attrs,
            visibility,
            kind: ast::BindingKind::Function {
                name: name_and_span,
                sig,
                body,
            },
            span: start_span.to(self.previous_span()),
        })
    }

    pub fn parse_extern_binding(
        &mut self,
        attrs: Vec<ast::Attr>,
        visibility: ast::Visibility,
    ) -> DiagnosticResult<ast::Binding> {
        let start_span = self.previous_span();

        if eat!(self, Fn) {
            let id = require!(self, Ident(_), "an identifier")?;
            let name = id.name();

            let name_and_span = ast::NameAndSpan { name, span: id.span };

            let sig = self.parse_function_sig(Some(name), true)?;

            Ok(ast::Binding {
                module_id: ModuleId::unknown(),
                attrs,
                visibility,
                kind: ast::BindingKind::ExternFunction {
                    name: name_and_span,
                    sig,
                },
                span: start_span.to(self.previous_span()),
            })
        } else if eat!(self, Let) {
            let is_mutable = eat!(self, Mut);

            let id = require!(self, Ident(_), "an identifier")?;
            let name = id.name();

            let name_and_span = ast::NameAndSpan { name, span: id.span };

            require!(self, Colon, ":")?;

            let type_expr = self.parse_expr()?;

            Ok(ast::Binding {
                module_id: ModuleId::unknown(),
                attrs,
                visibility,
                kind: ast::BindingKind::ExternVariable {
                    name: name_and_span,
                    is_mutable,
                    type_expr: Box::new(type_expr),
                },
                span: start_span.to(self.previous_span()),
            })
        } else {
            Err(SyntaxError::expected(self.span(), "fn or let"))
        }
    }

    pub fn parse_type_binding(
        &mut self,
        attrs: Vec<ast::Attr>,
        visibility: ast::Visibility,
    ) -> DiagnosticResult<ast::Binding> {
        let start_span = self.previous_span();

        let id = require!(self, Ident(_), "an identifier")?;
        let name = id.name();

        require!(self, Eq, "=")?;

        let type_expr = self.parse_named_expr(name)?;

        Ok(ast::Binding {
            module_id: ModuleId::unknown(),
            attrs,
            visibility,
            kind: ast::BindingKind::Type {
                name: ast::NameAndSpan { name, span: id.span },
                type_expr: Box::new(type_expr),
            },
            span: start_span.to(self.previous_span()),
        })
    }
}
