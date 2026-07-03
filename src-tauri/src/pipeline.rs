use std::collections::{HashMap, VecDeque};
use serde::{Deserialize, Serialize};
use crate::plugins;
use crate::logs::LogManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub label: Option<String>,
    pub data: serde_json::Value,
    pub position: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub source_handle: Option<String>,
    pub target_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub nodes: Vec<PipelineNode>,
    pub edges: Vec<PipelineEdge>,
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub node_id: String,
    pub node_label: String,
    pub node_type: String,
    pub input_count: usize,
    pub output_count: usize,
    pub detail: String,
    pub output: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub steps: Vec<ExecutionStep>,
    pub final_output: Vec<serde_json::Value>,
    pub error: Option<String>,
}

fn demo_sample_data() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"id": 1, "title": "CrawlFlow Demo", "description": "A visual web crawler configurator", "author": "CrawlFlow Team", "url": "https://crawlflow.ai", "tags": ["crawler", "visual", "tool"], "views": 1520}),
        serde_json::json!({"id": 2, "title": "Getting Started Guide", "description": "Learn how to use CrawlFlow in 5 minutes", "author": "Docs Team", "url": "https://crawlflow.ai/docs", "tags": ["guide", "tutorial"], "views": 890}),
        serde_json::json!({"id": 3, "title": "Plugin Development", "description": "Create your own Python plugins", "author": "Dev Team", "url": "https://crawlflow.ai/plugins", "tags": ["python", "plugin", "dev"], "views": 340}),
        serde_json::json!({"id": 4, "title": "Marketplace Launch", "description": "Browse and install community plugins", "author": "Community", "url": "https://crawlflow.ai/marketplace", "tags": ["marketplace", "community"], "views": 2100}),
        serde_json::json!({"id": 5, "title": "Architecture Overview", "description": "Deep dive into the CrawlFlow architecture", "author": "CrawlFlow Team", "url": "https://crawlflow.ai/architecture", "tags": ["architecture", "deep-dive"], "views": 670}),
    ]
}

fn topological_sort(
    nodes: &[PipelineNode],
    edges: &[PipelineEdge],
) -> Result<Vec<String>, String> {
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

    for node in nodes {
        in_degree.entry(node.id.clone()).or_insert(0);
        adjacency.entry(node.id.clone()).or_default();
    }

    for edge in edges {
        if let Some(degree) = in_degree.get_mut(&edge.target) {
            *degree += 1;
        }
        adjacency
            .entry(edge.source.clone())
            .or_default()
            .push(edge.target.clone());
    }

    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut order = Vec::new();
    while let Some(node_id) = queue.pop_front() {
        order.push(node_id.clone());
        if let Some(neighbors) = adjacency.get(&node_id) {
            for neighbor in neighbors {
                if let Some(degree) = in_degree.get_mut(neighbor) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    if order.len() != nodes.len() {
        return Err(format!(
            "Cycle detected: processed {} of {} nodes",
            order.len(),
            nodes.len()
        ));
    }

    Ok(order)
}

fn node_inputs(
    node_id: &str,
    edges: &[PipelineEdge],
    node_outputs: &HashMap<String, Vec<serde_json::Value>>,
) -> Vec<serde_json::Value> {
    let mut combined = Vec::new();
    for edge in edges {
        if edge.target == node_id {
            if let Some(output) = node_outputs.get(&edge.source) {
                combined.extend(output.iter().cloned());
            }
        }
    }
    combined
}

fn label_of(node: &PipelineNode) -> &str {
    node.label.as_deref().unwrap_or(&node.node_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, node_type: &str) -> PipelineNode {
        PipelineNode {
            id: id.to_string(),
            node_type: node_type.to_string(),
            label: None,
            data: serde_json::Value::Null,
            position: None,
        }
    }

    fn make_edge(id: &str, src: &str, tgt: &str) -> PipelineEdge {
        PipelineEdge {
            id: id.to_string(),
            source: src.to_string(),
            target: tgt.to_string(),
            source_handle: None,
            target_handle: None,
        }
    }

    #[test]
    fn test_topological_sort_simple() {
        let nodes = vec![make_node("a", "start"), make_node("b", "processor"), make_node("c", "export")];
        let edges = vec![make_edge("e1", "a", "b"), make_edge("e2", "b", "c")];
        let order = topological_sort(&nodes, &edges).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_topological_sort_disconnected() {
        let nodes = vec![make_node("a", "start"), make_node("b", "processor"), make_node("c", "export")];
        let edges = vec![];
        let order = topological_sort(&nodes, &edges).unwrap();
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn test_topological_sort_diamond() {
        let nodes = vec![
            make_node("a", "start"),
            make_node("b", "processor"),
            make_node("c", "processor"),
            make_node("d", "export"),
        ];
        let edges = vec![
            make_edge("e1", "a", "b"),
            make_edge("e2", "a", "c"),
            make_edge("e3", "b", "d"),
            make_edge("e4", "c", "d"),
        ];
        let order = topological_sort(&nodes, &edges).unwrap();
        assert_eq!(order.len(), 4);
        assert_eq!(order[0], "a");
        assert_eq!(order[3], "d");
    }

    #[test]
    fn test_topological_sort_cycle_detected() {
        let nodes = vec![make_node("a", "start"), make_node("b", "processor")];
        let edges = vec![make_edge("e1", "a", "b"), make_edge("e2", "b", "a")];
        let result = topological_sort(&nodes, &edges);
        assert!(result.is_err());
    }

    #[test]
    fn test_topological_sort_single_node() {
        let nodes = vec![make_node("a", "start")];
        let edges = vec![];
        let order = topological_sort(&nodes, &edges).unwrap();
        assert_eq!(order, vec!["a"]);
    }

    #[test]
    fn test_node_inputs_single_edge() {
        let node_outputs: HashMap<String, Vec<serde_json::Value>> = [
            ("a".to_string(), vec![serde_json::json!({"val": 1})]),
        ].into();
        let edges = vec![make_edge("e1", "a", "b")];
        let inputs = node_inputs("b", &edges, &node_outputs);
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0]["val"], 1);
    }

    #[test]
    fn test_node_inputs_multiple_edges() {
        let node_outputs: HashMap<String, Vec<serde_json::Value>> = [
            ("a".to_string(), vec![serde_json::json!({"val": 1})]),
            ("b".to_string(), vec![serde_json::json!({"val": 2})]),
        ].into();
        let edges = vec![make_edge("e1", "a", "c"), make_edge("e2", "b", "c")];
        let inputs = node_inputs("c", &edges, &node_outputs);
        assert_eq!(inputs.len(), 2);
    }

    #[test]
    fn test_node_inputs_no_edges() {
        let node_outputs: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        let edges = vec![];
        let inputs = node_inputs("a", &edges, &node_outputs);
        assert_eq!(inputs.len(), 0);
    }
}

pub fn execute_pipeline(
    config: &PipelineConfig,
    log_manager: &LogManager,
    project_id: &str,
) -> ExecutionResult {
    let mut steps = Vec::new();
    let mut node_outputs: HashMap<String, Vec<serde_json::Value>> = HashMap::new();

    let order = match topological_sort(&config.nodes, &config.edges) {
        Ok(o) => o,
        Err(e) => {
            return ExecutionResult {
                success: false,
                steps: vec![],
                final_output: vec![],
                error: Some(e),
            };
        }
    };

    log_manager.info(project_id, "pipeline", &format!("Pipeline execution order: {} nodes", order.len()));

    for node_id in &order {
        let node = match config.nodes.iter().find(|n| &n.id == node_id) {
            Some(n) => n,
            None => continue,
        };

        let input = node_inputs(node_id, &config.edges, &node_outputs);
        let input_count = input.len();

        let (output, detail) = match node.node_type.as_str() {
            "start" | "dataSource" | "rssSource" => {
                let items = demo_sample_data();
                let count = items.len();
                log_manager.info(
                    project_id,
                    &node_id,
                    &format!("[{}] Generated {} sample data items", label_of(node), count),
                );
                (items, format!("Generated {} sample items", count))
            }

            "repository" => {
                let count = input.len();
                log_manager.info(
                    project_id,
                    &node_id,
                    &format!("[{}] Stored {} items in repository", label_of(node), count),
                );
                (input, format!("Stored {} items", count))
            }

            "processor" => {
                let processor_type = node
                    .data
                    .get("processorType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let processor_config = node
                    .data
                    .get("processorConfig")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                let result = match processor_type {
                    "rust-deduplicate" => {
                        let cfg = processor_config;
                        plugins::deduplicate_plugin(input, cfg)
                    }
                    "rust-filter" => {
                        let cfg = processor_config;
                        plugins::filter_plugin(input, cfg)
                    }
                    "rust-sort" => {
                        let cfg = processor_config;
                        plugins::sort_plugin(input, cfg)
                    }
                    "rust-limit" => {
                        let cfg = processor_config;
                        plugins::limit_plugin(input, cfg)
                    }
                    _ => {
                        log_manager.warn(
                            project_id,
                            &node_id,
                            &format!("Unknown processor type: {}", processor_type),
                        );
                        Ok(input)
                    }
                };

                match result {
                    Ok(output) => {
                        let out_count = output.len();
                        log_manager.info(
                            project_id,
                            &node_id,
                            &format!(
                                "[{}] {}: {} → {} items",
                                label_of(node),
                                processor_type,
                                input_count,
                                out_count
                            ),
                        );
                        (output, format!("{}: {} → {} items", processor_type, input_count, out_count))
                    }
                    Err(e) => {
                        log_manager.error(
                            project_id,
                            &node_id,
                            &format!("[{}] Processor failed: {}", label_of(node), e),
                        );
                        (vec![], format!("Error: {}", e))
                    }
                }
            }

            "htmlExtractor" => {
                log_manager.info(
                    project_id,
                    &node_id,
                    &format!("[{}] Passed through {} items (extraction not yet implemented in service)", label_of(node), input_count),
                );
                (input, format!("Passed through {} items (extraction stubbed)", input_count))
            }

            "excelExport" => {
                let count = input.len();
                let sheet_name = node
                    .data
                    .get("sheetName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Sheet1");

                let result = plugins::excel_export_plugin(input.clone(), serde_json::json!({
                    "sheetName": sheet_name,
                    "fileName": format!("export_{}.xlsx", node.id),
                }));

                match result {
                    Ok(output) => {
                        log_manager.info(
                            project_id,
                            &node_id,
                            &format!("[{}] Exported {} items to Excel", label_of(node), count),
                        );
                        (output, format!("Exported {} items to Excel", count))
                    }
                    Err(e) => {
                        log_manager.error(
                            project_id,
                            &node_id,
                            &format!("[{}] Excel export failed: {}", label_of(node), e),
                        );
                        (vec![], format!("Excel export error: {}", e))
                    }
                }
            }

            "csvExport" | "databaseExport" => {
                let count = input.len();
                log_manager.info(
                    project_id,
                    &node_id,
                    &format!("[{}] Exporting {} items", label_of(node), count),
                );
                (input.clone(), format!("Exported {} items", count))
            }

            _ => {
                log_manager.debug(
                    project_id,
                    &node_id,
                    &format!("[{}] Unknown type '{}', passing through {} items", label_of(node), node.node_type, input_count),
                );
                (input, format!("Passed through {} items (unknown type)", input_count))
            }
        };

        let output_count = output.len();
        node_outputs.insert(node_id.clone(), output.clone());

        steps.push(ExecutionStep {
            node_id: node_id.clone(),
            node_label: label_of(node).to_string(),
            node_type: node.node_type.clone(),
            input_count,
            output_count,
            detail,
            output: output.clone(),
        });
    }

    let final_output = if let Some(last) = order.last() {
        node_outputs.get(last).cloned().unwrap_or_default()
    } else {
        vec![]
    };

    log_manager.info(
        project_id,
        "pipeline",
        &format!(
            "Pipeline complete: {} steps, final output: {} items",
            steps.len(),
            final_output.len()
        ),
    );

    ExecutionResult {
        success: true,
        steps,
        final_output,
        error: None,
    }
}
