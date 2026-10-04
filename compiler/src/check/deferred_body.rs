//! Generic bodies whose signatures supply initializer types before pass C.
use super::*;

#[derive(Clone)]
pub(super) struct PendingFunctionBody {
    source: ast::Function,
    name: String,
    class: Option<ClassId>,
    is_static: bool,
    pos: Pos,
    file: usize,
    subst: HashMap<String, Type>,
    chain: Vec<instance_chain::InstanceRequest>,
}

impl Checker<'_> {
    pub(super) fn defer_function_body(
        &mut self,
        source: &ast::Function,
        name: &str,
        class: Option<ClassId>,
        is_static: bool,
        pos: &Pos,
    ) {
        self.pending_function_bodies.push(PendingFunctionBody {
            source: source.clone(),
            name: name.to_string(),
            class,
            is_static,
            pos: pos.clone(),
            file: self.cur_file,
            subst: self.subst.clone(),
            chain: self.instance_chain.clone(),
        });
    }

    pub(super) fn check_pending_function_bodies(&mut self) {
        while !self.pending_function_bodies.is_empty() {
            for pending in std::mem::take(&mut self.pending_function_bodies) {
                let file = std::mem::replace(&mut self.cur_file, pending.file);
                let subst = std::mem::replace(&mut self.subst, pending.subst);
                let chain = std::mem::replace(&mut self.instance_chain, pending.chain);
                let diagnostics = self.diags.len();
                let sig = if let Some(class) = pending.class {
                    self.decide_class_parameters(class);
                    if pending.is_static {
                        self.fn_sigs.get(&pending.name).cloned()
                    } else {
                        self.class_sigs[class.0].methods.get(&pending.name).cloned()
                    }
                } else {
                    self.decide_function_parameters(&pending.name);
                    self.fn_sigs.get(&pending.name).cloned()
                };
                if let Some(sig) = sig {
                    if let Some(function) = self.check_function(
                        &pending.source,
                        &pending.name,
                        false,
                        &sig,
                        (
                            pending.class.map(Type::Class),
                            pending.is_static.then_some(RejectionSite::ThisStaticField),
                        ),
                        pending.pos,
                    ) {
                        if let Some(class) = pending.class.filter(|_| !pending.is_static) {
                            self.classes[class.0].methods.push(function);
                        } else {
                            self.functions.push(function);
                        }
                    }
                }
                self.instance_diagnostic_ranges
                    .push(diagnostics..self.diags.len());
                self.cur_file = file;
                self.subst = subst;
                self.instance_chain = chain;
            }
        }
    }
}
