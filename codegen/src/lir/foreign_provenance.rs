use super::*;

pub(super) fn convert_provenance(value: &hir::ForeignTypeProvenance) -> l::ForeignTypeProvenance {
    match value {
        hir::ForeignTypeProvenance::Descriptor {
            aggregate,
            element,
            element_const,
        } => l::ForeignTypeProvenance::Descriptor {
            aggregate: aggregate.clone(),
            element: element.clone(),
            element_const: *element_const,
        },
        hir::ForeignTypeProvenance::ScalarPair {
            element,
            element_const,
        } => l::ForeignTypeProvenance::ScalarPair {
            element: element.clone(),
            element_const: *element_const,
        },
        hir::ForeignTypeProvenance::StringView { aggregate } => {
            l::ForeignTypeProvenance::StringView {
                aggregate: aggregate.clone(),
            }
        }
        hir::ForeignTypeProvenance::Callback {
            typedef_name,
            host_view,
            view_members,
        } => l::ForeignTypeProvenance::Callback {
            typedef_name: typedef_name.clone(),
            host_view: host_view.clone(),
            view_members: view_members.clone(),
        },
        hir::ForeignTypeProvenance::PairOrder {
            pointer_first,
            element_const,
        } => l::ForeignTypeProvenance::PairOrder {
            pointer_first: *pointer_first,
            element_const: *element_const,
        },
        hir::ForeignTypeProvenance::ConstPointer => l::ForeignTypeProvenance::ConstPointer,
        _ => unreachable!("new foreign provenance requires an explicit LIR form"),
    }
}
