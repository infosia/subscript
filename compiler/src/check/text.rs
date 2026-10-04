//! Error formatting and URI checking (stdlib.md §19).

use super::exception::ErrorKind;
use super::{Checker, FnCtx, ParamSig};
use crate::check::rejection::RejectionSite;
use crate::diag::Pos;
use crate::hir::{self, BinOp, Callee, ExprKind, TextFn};
use crate::types::Type;
use swc_ecma_ast as ast;

pub(crate) fn uri_function(name: &str) -> Option<TextFn> {
    Some(match name {
        "encodeURI" => TextFn::EncodeUri,
        "encodeURIComponent" => TextFn::EncodeComponent,
        "decodeURI" => TextFn::DecodeUri,
        "decodeURIComponent" => TextFn::DecodeComponent,
        _ => return None,
    })
}

fn call(function: TextFn, args: Vec<hir::Expr>, pos: &Pos) -> hir::Expr {
    hir::Expr {
        pending_work: None,
        kind: ExprKind::Call {
            callee: Callee::Text(function),
            args,
        },
        ty: Type::Str,
        pos: pos.clone(),
    }
}

fn local(name: &str, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        pending_work: None,
        kind: ExprKind::Local(name.into(), ty.clone()),
        ty,
        pos: pos.clone(),
    }
}

impl Checker<'_> {
    pub(crate) fn check_uri_call(
        &mut self,
        function: TextFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        name: &str,
    ) -> hir::Expr {
        if c.type_args.is_some() {
            self.reject_subset(
                RejectionSite::UriCodecTypeArguments,
                format!("`{name}` is not generic"),
                pos.clone(),
            );
        }
        let args = self.check_args(
            RejectionSite::UriCodecArgumentCount,
            &[ParamSig::positional(Type::Str)],
            &c.args,
            fx,
            &pos,
            name,
        );
        let failure = match function {
            TextFn::DecodeUri => TextFn::UriFailure,
            TextFn::DecodeComponent => TextFn::ComponentFailure,
            _ => return call(function, args, &pos),
        };
        let Some(kind) = ErrorKind::from_name("URIError") else {
            return self.err_expr(pos);
        };
        let error = self.error_new(kind, local("message", Type::Str, &pos), pos.clone());
        let body = vec![
            hir::Stmt::Let {
                name: "message".into(),
                ty: Type::Str,
                mutable: false,
                dispose: false,
                init: call(failure, vec![local("value", Type::Str, &pos)], &pos),
                pos: pos.clone(),
            },
            hir::Stmt::If {
                cond: hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Binary {
                        op: BinOp::Ne,
                        left: Box::new(local("message", Type::Str, &pos)),
                        right: Box::new(hir::Expr {
                            pending_work: None,
                            kind: ExprKind::Str(String::new()),
                            ty: Type::Str,
                            pos: pos.clone(),
                        }),
                    },
                    ty: Type::Bool,
                    pos: pos.clone(),
                },
                then: vec![hir::Stmt::Throw {
                    value: error,
                    pos: pos.clone(),
                }],
                els: None,
                pos: pos.clone(),
            },
            hir::Stmt::Return {
                value: Some(call(function, vec![local("value", Type::Str, &pos)], &pos)),
                pos: pos.clone(),
            },
        ];
        self.text_helper(name, Type::Str, args, body, pos)
    }

    pub(crate) fn error_to_string(&mut self, recv: hir::Expr, pos: Pos) -> hir::Expr {
        let field = |name: &str| hir::Expr {
            pending_work: None,
            kind: ExprKind::Field {
                obj: Box::new(local("value", recv.ty.clone(), &pos)),
                name: name.into(),
            },
            ty: Type::Str,
            pos: pos.clone(),
        };
        let body = vec![hir::Stmt::Return {
            value: Some(call(
                TextFn::ErrorToString,
                vec![field("name"), field("message")],
                &pos,
            )),
            pos: pos.clone(),
        }];
        self.text_helper("Error.toString", recv.ty.clone(), vec![recv], body, pos)
    }

    fn text_helper(
        &mut self,
        name: &str,
        parameter_type: Type,
        args: Vec<hir::Expr>,
        body: Vec<hir::Stmt>,
        pos: Pos,
    ) -> hir::Expr {
        let name = format!("[[{name}#{}]]", self.functions.len());
        self.functions.push(hir::Function::new_synthesized_helper(
            name.clone(),
            vec![hir::Param {
                name: "value".into(),
                ty: parameter_type,
                escapes: false,
                default_can_raise: false,
                default: None,
                foreign_provenance: None,
                pos: pos.clone(),
            }],
            Type::Str,
            body,
            pos.clone(),
        ));
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Func(hir::Symbol::from_full_text(name)),
                args,
            },
            ty: Type::Str,
            pos,
        }
    }
}
