//! Source identities are recovered from the exact evaluated document, never lane order.
use music_link::{Binding, Definition, NamedPart, Snapshot, named_color};
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

pub fn enrich(snapshot: &mut Snapshot) {
    if snapshot.bindings.is_empty() {
        snapshot.bindings = bindings(&snapshot.source);
    }
    if snapshot.definitions.is_empty() {
        for binding in &snapshot.bindings {
            if snapshot.definitions.iter().any(|d| d.name == binding.name) {
                continue;
            }
            snapshot.definitions.push(Definition {
                name: binding.name.clone(),
                color: binding.color,
                ..Default::default()
            });
        }
    }
    for event in &mut snapshot.events {
        if event.part.is_empty() {
            let identity = event
                .context
                .iter()
                .flat_map(|(from, to)| {
                    snapshot.bindings.iter().filter(move |b| {
                        *from >= b.definition_from && *to <= b.definition_to && from < to
                    })
                })
                .min_by_key(|b| b.definition_to - b.definition_from);
            if let Some(binding) = identity {
                event.part = binding.name.clone();
                event.part_color = binding.color;
            }
        }
        if !event.part.is_empty()
            && let Some(lane) = snapshot.lanes.iter_mut().find(|l| l.id == event.lane)
            && !lane.parts.iter().any(|p| p.name == event.part)
        {
            lane.parts.push(NamedPart {
                name: event.part.clone(),
                color: event.part_color,
                helper: false,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadowed_parameters_are_not_colored_as_score_constants() {
        let source = "const bass = note(\"c3\"); const f = bass => bass; $: bass;";
        assert_eq!(
            bindings(source).iter().filter(|b| b.name == "bass").count(),
            2
        );
    }
    #[test]
    fn exact_utf8_context_colors_named_parts_and_uses() {
        let source = "// const fake = 0\nconst mélodie = note(\"c3\");\n$: mélodie.slow(2);";
        let start = source.find("c3").unwrap();
        let mut s = Snapshot {
            source: source.into(),
            lanes: vec![music_link::Lane {
                id: "#0".into(),
                ..Default::default()
            }],
            events: vec![music_link::Event {
                lane: "#0".into(),
                context: vec![(start, start + 2)],
                ..crate::model::test_event()
            }],
            ..Default::default()
        };
        enrich(&mut s);
        assert_eq!(s.definitions.len(), 1);
        assert_eq!(s.bindings.len(), 2);
        assert_eq!(s.events[0].part, "mélodie");
        assert_eq!(s.events[0].part_color, named_color("mélodie"));
        assert_eq!(s.lanes[0].parts[0].name, "mélodie");
        assert!(
            s.bindings
                .iter()
                .all(|b| &source[b.from..b.to] == "mélodie")
        );
    }
}
