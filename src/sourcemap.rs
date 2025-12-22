use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcemapNode {
    pub name: String,
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_paths: Option<Vec<PathBuf>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SourcemapNode>,
}

impl SourcemapNode {
    pub fn new(name: &str, class_name: &str) -> Self {
        Self {
            name: name.to_string(),
            class_name: class_name.to_string(),
            file_paths: None,
            children: Vec::new(),
        }
    }
}

/// Generates a sourcemap JSON string by walking the 'game' directory.
pub fn generate_sourcemap(root_path: PathBuf) -> anyhow::Result<String> {
    let game_path = root_path.join("game");
    let mut root = SourcemapNode::new("Project", "DataModel");

    if game_path.exists() && game_path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(game_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let dir_name = entry.file_name().to_string_lossy().to_string();

                    // Map top-level directories to Roblox Services
                    let class_name = match dir_name.as_str() {
                        "ServerScriptService" => "ServerScriptService",
                        "ReplicatedStorage" => "ReplicatedStorage",
                        "StarterPlayer" => "StarterPlayer",
                        "StarterGui" => "StarterGui",
                        "ReplicatedFirst" => "ReplicatedFirst",
                        "SoundService" => "SoundService",
                        "Chat" => "Chat",
                        "Lighting" => "Lighting",
                        "MaterialService" => "MaterialService",
                        "HttpService" => "HttpService",
                        "Workspace" => "Workspace",
                        _ => "Folder",
                    };

                    if let Some(node) = walk_directory(&path, &dir_name, class_name) {
                        root.children.push(node);
                    }
                }
            }
        }
    }

    let json = serde_json::to_string_pretty(&root)?;
    Ok(json)
}

fn walk_directory(dir_path: &Path, name: &str, class_name: &str) -> Option<SourcemapNode> {
    if !dir_path.exists() || !dir_path.is_dir() {
        return None;
    }

    let mut node = SourcemapNode::new(name, class_name);

    // Check for init scripts to associate with this folder node
    // Priority: init.server.luau (Script), init.client.luau (LocalScript), init.luau (ModuleScript)
    let init_server = dir_path.join("init.server.luau");
    let init_client = dir_path.join("init.client.luau");
    let init_module = dir_path.join("init.luau");

    if init_server.exists() {
        node.file_paths = Some(vec![init_server]);
        if class_name == "Folder" { node.class_name = "Script".to_string(); }
    } else if init_client.exists() {
        node.file_paths = Some(vec![init_client]);
        if class_name == "Folder" { node.class_name = "LocalScript".to_string(); }
    } else if init_module.exists() {
        node.file_paths = Some(vec![init_module]);
        if class_name == "Folder" { node.class_name = "ModuleScript".to_string(); }
    }

    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                // Handle special sub-folders in services
                let child_class = if name == "StarterPlayer" && file_name == "StarterPlayerScripts" {
                    "StarterPlayerScripts"
                } else if name == "StarterPlayer" && file_name == "StarterCharacterScripts" {
                    "StarterCharacterScripts"
                } else {
                    "Folder"
                };

                if let Some(child_node) = walk_directory(&path, &file_name, child_class) {
                    node.children.push(child_node);
                }
            } else if path.is_file() {
                // Skip init scripts as they are handled by the parent folder
                if file_name.starts_with("init.") {
                    continue;
                }

                if file_name.ends_with(".luau") {
                    let (name_part, class_part) = parse_script_name(&file_name);
                    let mut child = SourcemapNode::new(&name_part, &class_part);
                    child.file_paths = Some(vec![path]);
                    node.children.push(child);
                }
            }
        }
    }

    Some(node)
}

fn parse_script_name(filename: &str) -> (String, String) {
    if let Some(stem) = filename.strip_suffix(".server.luau") {
        (stem.to_string(), "Script".to_string())
    } else if let Some(stem) = filename.strip_suffix(".client.luau") {
        (stem.to_string(), "LocalScript".to_string())
    } else if let Some(stem) = filename.strip_suffix(".luau") {
        (stem.to_string(), "ModuleScript".to_string())
    } else {
        (filename.to_string(), "Folder".to_string())
    }
}
