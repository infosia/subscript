//! Resolves the four file-module forms before HIR construction.
use super::*;
use crate::hir::StandardHostOperation as Op;

pub(super) const FORMS: &str = "accepted forms: readFile(path: string, encoding: \"utf8\"): Promise<string>; readFile(path: string): Promise<u8[]>; writeFile(path: string, data: string): Promise<void>; writeFile(path: string, data: u8[]): Promise<void>";

impl Checker<'_> {
    pub(super) fn file_error(&self, site: RejectionSite, message: &str, pos: Pos) {
        self.diags.push(rejection::diagnostic(site, message, pos));
    }
    pub(super) fn check_file_call(
        &mut self,
        write: bool,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let invalid = call.type_args.is_some()
            || call.args.iter().any(|a| a.spread.is_some())
            || if write {
                call.args.len() != 2
            } else {
                !(1..=2).contains(&call.args.len())
            };
        if invalid {
            self.file_error(RejectionSite::FileModuleArguments, FORMS, pos.clone());
            return self.err_expr(pos);
        }
        let path = self.check_expr(&call.args[0].expr, Some(&Type::Str), fx);
        let path = self.apparent_expr(path);
        // tsc accepts a string-literal union alias for `path` (rule 5a).
        let site = match &path.ty {
            Type::Str | Type::Error => None,
            Type::StringAlias(_) => Some(RejectionSite::FileModuleStringAlias),
            _ => Some(RejectionSite::FileModuleArguments),
        };
        if let Some(site) = site {
            self.file_error(site, FORMS, path.pos.clone());
            return self.err_expr(pos);
        }
        let mut args = vec![path];
        let op = if write {
            let byte_array = Type::Array(Box::new(Type::U8));
            let expected =
                matches!(&*call.args[1].expr, ast::Expr::Array(_)).then_some(&byte_array);
            let data = self.check_expr(&call.args[1].expr, expected, fx);
            let data = self.apparent_expr(data);
            let op = match &data.ty {
                Type::Str => Op::WriteText,
                Type::Array(e) if self.apparent_type(e) == Type::U8 => Op::WriteBytes,
                Type::Error => return self.err_expr(pos),
                // tsc reads every number type as `number`, and accepts a
                // string-literal union alias as a string (rule 5a).
                Type::Array(e) if self.apparent_type(e).is_numeric() => {
                    self.file_error(
                        RejectionSite::FileModuleNumberArray,
                        FORMS,
                        data.pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                Type::StringAlias(_) => {
                    self.file_error(
                        RejectionSite::FileModuleStringAlias,
                        FORMS,
                        data.pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                _ => {
                    self.file_error(RejectionSite::FileModuleArguments, FORMS, data.pos.clone());
                    return self.err_expr(pos);
                }
            };
            args.push(data);
            op
        } else if call.args.len() == 1 {
            Op::ReadBytes
        } else {
            let encoding = &*call.args[1].expr;
            match encoding {
                ast::Expr::Lit(ast::Lit::Str(s)) if s.value == *"utf8" => {}
                ast::Expr::Lit(_) | ast::Expr::Object(_) => {
                    self.file_error(
                        RejectionSite::FileModuleArguments,
                        FORMS,
                        self.pos(encoding.span()),
                    );
                    return self.err_expr(pos);
                }
                _ => {
                    // A string expression that is not the literal: tsc
                    // accepts one whose type is the literal "utf8".
                    let value = self.check_expr(encoding, Some(&Type::Str), fx);
                    let value = self.apparent_expr(value);
                    let site = if value.ty == Type::Str {
                        RejectionSite::FileModuleEncoding
                    } else {
                        RejectionSite::FileModuleArguments
                    };
                    if value.ty != Type::Error {
                        self.file_error(site, FORMS, value.pos.clone());
                    }
                    return self.err_expr(pos);
                }
            }
            Op::ReadText
        };
        let value = hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Call {
                callee: hir::Callee::Standard(op),
                args,
            },
            ty: Type::async_handle(op.result()),
            pos,
        };
        self.track_async_call_result(value, fx)
    }
}
