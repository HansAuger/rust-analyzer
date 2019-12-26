//! Defines hir-level representation of visibility (e.g. `pub` and `pub(crate)`).

use hir_expand::{hygiene::Hygiene, InFile};
use ra_syntax::ast;

use crate::{
    db::DefDatabase,
    path::{ModPath, PathKind},
    ModuleId,
};

/// Visibility of an item, not yet resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawVisibility {
    /// `pub(in module)`, `pub(crate)` or `pub(super)`. Also private, which is
    /// equivalent to `pub(self)`.
    Module(ModPath),
    /// `pub`.
    Public,
}

impl RawVisibility {
    const fn private() -> RawVisibility {
        let path = ModPath { kind: PathKind::Super(0), segments: Vec::new() };
        RawVisibility::Module(path)
    }

    pub(crate) fn from_ast(
        db: &impl DefDatabase,
        node: InFile<Option<ast::Visibility>>,
    ) -> RawVisibility {
        Self::from_ast_with_hygiene(node.value, &Hygiene::new(db, node.file_id))
    }

    pub(crate) fn from_ast_with_hygiene(
        node: Option<ast::Visibility>,
        hygiene: &Hygiene,
    ) -> RawVisibility {
        let node = match node {
            None => return RawVisibility::private(),
            Some(node) => node,
        };
        match node.kind() {
            ast::VisibilityKind::In(path) => {
                let path = ModPath::from_src(path, hygiene);
                let path = match path {
                    None => return RawVisibility::private(),
                    Some(path) => path,
                };
                RawVisibility::Module(path)
            }
            ast::VisibilityKind::PubCrate => {
                let path = ModPath { kind: PathKind::Crate, segments: Vec::new() };
                RawVisibility::Module(path)
            }
            ast::VisibilityKind::PubSuper => {
                let path = ModPath { kind: PathKind::Super(1), segments: Vec::new() };
                RawVisibility::Module(path)
            }
            ast::VisibilityKind::Pub => RawVisibility::Public,
        }
    }

    pub fn resolve(
        &self,
        db: &impl DefDatabase,
        resolver: &crate::resolver::Resolver,
    ) -> Visibility {
        // we fall back to public visibility (i.e. fail open) if the path can't be resolved
        resolver.resolve_visibility(db, self).unwrap_or(Visibility::Public)
    }
}

/// Visibility of an item, with the path resolved.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Visibility {
    /// Visibility is restricted to a certain module.
    Module(ModuleId),
    /// Visibility is unrestricted.
    Public,
}

impl Visibility {
    pub fn visible_from(self, db: &impl DefDatabase, from_module: ModuleId) -> bool {
        let to_module = match self {
            Visibility::Module(m) => m,
            Visibility::Public => return true,
        };
        // if they're not in the same crate, it can't be visible
        if from_module.krate != to_module.krate {
            return false;
        }
        let def_map = db.crate_def_map(from_module.krate);
        self.visible_from_def_map(&def_map, from_module.local_id)
    }

    pub(crate) fn visible_from_other_crate(self) -> bool {
        match self {
            Visibility::Module(_) => false,
            Visibility::Public => true,
        }
    }

    pub(crate) fn visible_from_def_map(
        self,
        def_map: &crate::nameres::CrateDefMap,
        from_module: crate::LocalModuleId,
    ) -> bool {
        let to_module = match self {
            Visibility::Module(m) => m,
            Visibility::Public => return true,
        };
        // from_module needs to be a descendant of to_module
        let mut ancestors = std::iter::successors(Some(from_module), |m| {
            let parent_id = def_map[*m].parent?;
            Some(parent_id)
        });
        ancestors.any(|m| m == to_module.local_id)
    }
}
