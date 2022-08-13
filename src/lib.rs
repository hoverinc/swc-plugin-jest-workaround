mod local_export_strip;
mod utils;

use local_export_strip::LocalExportStrip;
use swc_common::{collections::AHashSet, util::take::Take, Mark, DUMMY_SP};
use swc_core::{
    ast::*,
    plugin::{metadata::TransformPluginProgramMetadata, plugin_transform},
    utils::{quote_ident, ExprFactory, IntoIndirectCall},
    visit::{as_folder, noop_visit_mut_type, FoldWith, VisitMut, VisitMutWith},
};

use utils::emit_export_stmts;

#[derive(Debug)]
pub struct TransformVisitor {
    unresolved_mark: Mark,

    export_decl_id: AHashSet<Id>,
}

impl VisitMut for TransformVisitor {
    noop_visit_mut_type!();

    fn visit_mut_module_items(&mut self, n: &mut Vec<ModuleItem>) {
        let mut strip = LocalExportStrip::default();
        n.visit_mut_with(&mut strip);

        let LocalExportStrip {
            has_export_assign,
            export,
            export_decl_id,
        } = strip;

        self.export_decl_id = export_decl_id;

        let mut stmts: Vec<ModuleItem> = Vec::with_capacity(n.len() + 1);

        if !has_export_assign && !export.is_empty() {
            // keep module env
            stmts.push(ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(
                NamedExport::dummy(),
            )));

            let exports = self.exports();

            let export_obj_prop_list = export.into_iter().map(Into::into).collect();

            stmts.extend(
                emit_export_stmts(exports, export_obj_prop_list)
                    .into_iter()
                    .map(Into::into),
            );

            if !self.export_decl_id.is_empty() {
                n.visit_mut_children_with(self);
            }
        }

        stmts.extend(n.take());

        *n = stmts;
    }

    fn visit_mut_expr(&mut self, n: &mut Expr) {
        match n {
            Expr::Ident(ref_ident) => {
                if self.export_decl_id.contains(&ref_ident.to_id()) {
                    *n = self.exports().make_member(ref_ident.take());
                }
            }

            _ => n.visit_mut_children_with(self),
        };
    }

    fn visit_mut_pat_or_expr(&mut self, n: &mut PatOrExpr) {
        match n {
            PatOrExpr::Pat(pat) if pat.is_ident() => {
                let ref_ident = pat.clone().expect_ident().id;
                if self.export_decl_id.contains(&ref_ident.to_id()) {
                    *n = PatOrExpr::Expr(Box::new(self.exports().make_member(ref_ident)));
                }
            }
            _ => {
                n.visit_mut_children_with(self);
            }
        }
    }

    fn visit_mut_callee(&mut self, n: &mut Callee) {
        match n {
            Callee::Expr(e) if e.is_ident() => {
                let is_indirect_callee = e
                    .as_ident()
                    .map(|ident| self.export_decl_id.contains(&ident.to_id()))
                    .unwrap_or_default();

                e.visit_mut_with(self);

                if is_indirect_callee {
                    *n = n.take().into_indirect()
                }
            }

            _ => n.visit_mut_children_with(self),
        }
    }
}

impl TransformVisitor {
    pub fn new(unresolved_mark: Mark) -> Self {
        Self {
            unresolved_mark,
            export_decl_id: Default::default(),
        }
    }

    fn exports(&self) -> Ident {
        quote_ident!(DUMMY_SP.apply_mark(self.unresolved_mark), "exports")
    }
}

#[plugin_transform]
pub fn process_transform(program: Program, metadata: TransformPluginProgramMetadata) -> Program {
    program.fold_with(&mut as_folder(TransformVisitor::new(
        metadata.unresolved_mark,
    )))
}
