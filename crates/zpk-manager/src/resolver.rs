//! Offline resolver: installed versions plus one candidate, never apk or network.
use crate::{fail, model::Record, Result};
use std::collections::{BTreeMap, BTreeSet};
use zpk_schema::Manifest;
pub fn resolve(
    installed: &BTreeMap<String, Record>,
    candidate: Option<&Manifest>,
    remove: Option<&str>,
) -> Result<()> {
    let mut graph = BTreeMap::new();
    for (id, r) in installed {
        if Some(id.as_str()) != remove {
            graph.insert(id.clone(), r.current().parsed()?);
        }
    }
    if let Some(m) = candidate {
        graph.insert(m.package.id.clone(), m.clone());
    }
    for m in graph.values() {
        if !m.compatibility.replaces.is_empty() {
            return fail("automatic replaces is unsupported; explicitly uninstall the replaced package first");
        }
        for conflict in &m.compatibility.conflicts {
            if graph.contains_key(conflict) {
                return fail(format!(
                    "package conflict: {} with {conflict}",
                    m.package.id
                ));
            }
        }
        for dep in &m.runtime.dependencies {
            let target = graph.get(&dep.id).ok_or_else(|| {
                format!(
                    "missing ZPK dependency: {} (required by {})",
                    dep.id, m.package.id
                )
            })?;
            if !dep.version.matches(&target.package.version) {
                return fail(format!(
                    "dependency {} requires {}, found {}",
                    dep.id, dep.version, target.package.version
                ));
            }
        }
    }
    fn visit(
        id: &str,
        graph: &BTreeMap<String, Manifest>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.into()) {
            return fail(format!("dependency cycle through {id}"));
        }
        for d in &graph[id].runtime.dependencies {
            visit(&d.id, graph, active, done)?;
        }
        active.remove(id);
        done.insert(id.into());
        Ok(())
    }
    let mut done = BTreeSet::new();
    let mut active = BTreeSet::new();
    for id in graph.keys() {
        visit(id, &graph, &mut active, &mut done)?;
    }
    Ok(())
}
