//! Private identifiers become private members named with `#` (compiler.md §206 and §341 rule 5).
use oxc_allocator::{Allocator, TakeIn};
use oxc_ast::ast;
use oxc_ast::builder::AstBuilder;
use oxc_ast_visit::{walk_mut, VisitMut};

/// The ordinary member name of a private identifier.
pub(crate) fn member_name(name: &ast::PrivateIdentifier<'_>) -> String {
    format!("#{}", name.name)
}

/// True for the reserved member spelling of a private identifier.
pub(crate) fn is_private_name(name: &str) -> bool {
    name.starts_with('#')
}

/// Lowers private names except optional-chain members and private `in` operands.
pub(crate) fn lower_module<'a>(allocator: &'a Allocator, module: &mut ast::Program<'a>) {
    PrivateNames { allocator }.visit_program(module);
}

struct PrivateNames<'a> {
    allocator: &'a Allocator,
}
impl<'a> PrivateNames<'a> {
    fn key(&self, key: &mut ast::PropertyKey<'a>) -> bool {
        if let ast::PropertyKey::PrivateIdentifier(name) = key {
            *key = ast::PropertyKey::StaticIdentifier(oxc_allocator::Box::new_in(
                self.name(name),
                &self.allocator,
            ));
            true
        } else {
            false
        }
    }
    fn name(&self, name: &ast::PrivateIdentifier<'a>) -> ast::IdentifierName<'a> {
        ast::IdentifierName::new(
            name.span,
            self.allocator.alloc_str(&member_name(name)),
            &AstBuilder::new(self.allocator),
        )
    }
    fn member(&mut self, member: &mut ast::MemberExpression<'a>) {
        if let ast::MemberExpression::PrivateFieldExpression(field) = member {
            self.visit_expression(&mut field.object);
            if !field.optional {
                // A chain's direct member stays private, even when only its object is optional (§206 rule 6).
                *member = ast::MemberExpression::StaticMemberExpression(
                    ast::StaticMemberExpression::boxed(
                        field.span,
                        field.object.take_in(&self.allocator),
                        self.name(&field.field),
                        false,
                        &AstBuilder::new(self.allocator),
                    ),
                );
            }
        } else {
            walk_mut::walk_member_expression(self, member);
        }
    }
}
impl<'a> VisitMut<'a> for PrivateNames<'a> {
    fn visit_class_element(&mut self, element: &mut ast::ClassElement<'a>) {
        let member = match element {
            ast::ClassElement::MethodDefinition(method) => {
                let method = &mut **method;
                Some((&mut method.key, &mut method.accessibility))
            }
            ast::ClassElement::PropertyDefinition(property) => {
                let property = &mut **property;
                Some((&mut property.key, &mut property.accessibility))
            }
            _ => None,
        };
        if let Some((key, accessibility)) = member {
            if self.key(key) {
                *accessibility = Some(ast::TSAccessibility::Private);
            }
        }
        walk_mut::walk_class_element(self, element);
    }
    fn visit_expression(&mut self, expression: &mut ast::Expression<'a>) {
        if let ast::Expression::ChainExpression(chain) = expression {
            // The direct private chain member keeps its kind; nested object expressions still lower.
            if let ast::ChainElement::PrivateFieldExpression(field) = &mut chain.expression {
                self.visit_expression(&mut field.object);
                return;
            }
        }
        if let Some(member) = expression.as_member_expression_mut() {
            self.member(member);
        } else {
            walk_mut::walk_expression(self, expression);
        }
    }
    fn visit_assignment_target(&mut self, target: &mut ast::AssignmentTarget<'a>) {
        if let Some(member) = target.as_member_expression_mut() {
            self.member(member);
        } else {
            walk_mut::walk_assignment_target(self, target);
        }
    }
    fn visit_simple_assignment_target(&mut self, target: &mut ast::SimpleAssignmentTarget<'a>) {
        if let Some(member) = target.as_member_expression_mut() {
            self.member(member);
        } else {
            walk_mut::walk_simple_assignment_target(self, target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "
class C {
  #a: i32 = 1;
  static #s: i32 = 2;
  b: i32 = this.#a;
  #m(): i32 { return this.#a; }
  get #g(): i32 { return C.#s; }
  set #g(v: i32) { this.#a = v; }
  constructor(p: i32 = C.#s) { this.#a = p; }
  f(o: C): void {
    const k = (): i32 => o.#a + [o.#a][0] + { v: o.#a }.v;
    for (let i = o.#a; i < o.#a; i++) { o.#a += `${o.#m()}`.length; }
    if (o.#a > 0) { o.#a++; } else { switch (o.#a) { case 1: break; } }
    try { throw new Error(`${o.#g}`); } catch (e) { o.#g = o.#a ? 1 : 2; }
    k();
  }
  h(o: C | null): boolean { return o?.#a === 1 && #a in o; }
}
export default class { #d: i32 = 3; e(): i32 { return this.#d; } }
";

    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn the_walk_lowers_each_private_name_outside_a_chain_and_an_in_operand() {
        let allocator = Allocator::default();
        let mut parsed = oxc_parser::Parser::new(
            &allocator,
            SOURCE,
            oxc_span::SourceType::ts().with_module(true),
        )
        .parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(
            format!("{:?}", parsed.program)
                .matches("PrivateIdentifier {")
                .count()
                > 20
        );
        lower_module(&allocator, &mut parsed.program);
        let text = format!("{:?}", parsed.program);
        assert_eq!(text.matches("PrivateIdentifier {").count(), 2, "{text}");
        assert!(text.contains("name: \"#a\""));
        assert!(text.contains("name: \"#d\""));
    }
}
