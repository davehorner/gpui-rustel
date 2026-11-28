//! Source identities shared by the native stage and browser Studio.
#[path = "../vendor/music-link/src/colors.rs"]
mod colors;
use colors::named_color;
#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub color: [u8; 3],
    pub from: usize,
    pub to: usize,
    pub definition_from: usize,
    pub definition_to: usize,
}
use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, Statement};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::SourceType;

pub fn bindings(source: &str) -> Vec<Binding> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    if parsed.panicked || !parsed.diagnostics.is_empty() {
        return vec![];
    }
    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    let mut result = vec![];
    for statement in &parsed.program.body {
        let Statement::VariableDeclaration(declaration) = statement else {
            continue;
        };
        for item in &declaration.declarations {
            let BindingPattern::BindingIdentifier(name) = &item.id else {
                continue;
            };
            result.push(Binding {
                name: name.name.to_string(),
                color: named_color(name.name.as_str()),
                from: name.span.start as usize,
                to: name.span.end as usize,
                definition_from: item.span.start as usize,
                definition_to: item.span.end as usize,
            });
        }
    }
    // Reference spans let the editor color uses as well as declarations. The semantic
    // identity of emitted events comes from their runtime source contexts below.
    struct References<'r> {
        declarations: &'r [Binding],
        scoping: &'r Scoping,
        uses: Vec<Binding>,
    }
    impl<'a> Visit<'a> for References<'_> {
        fn visit_identifier_reference(&mut self, id: &oxc_ast::ast::IdentifierReference<'a>) {
            if id
                .reference_id
                .get()
                .and_then(|id| self.scoping.get_reference(id).symbol_id())
                .is_some_and(|symbol| {
                    self.scoping.symbol_scope_id(symbol) == self.scoping.root_scope_id()
                })
                && let Some(binding) = self
                    .declarations
                    .iter()
                    .find(|b| b.name == id.name.as_str())
            {
                self.uses.push(Binding {
                    from: id.span.start as usize,
                    to: id.span.end as usize,
                    ..binding.clone()
                });
            }
            walk::walk_identifier_reference(self, id);
        }
    }
    let mut references = References {
        declarations: &result,
        scoping: semantic.scoping(),
        uses: vec![],
    };
    references.visit_program(&parsed.program);
    let uses = references.uses;
    result.extend(uses);
    result
}
