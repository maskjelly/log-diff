use std::collections::{HashMap, HashSet};

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use crate::command::diff::theme;
use crate::command::diff::types::{FileStatus, SidebarItem};

#[derive(Default)]
struct DirectoryStats {
    files: usize,
    viewed: usize,
    added: usize,
    modified: usize,
    deleted: usize,
}

fn item_depth(item: &SidebarItem) -> usize {
    match item {
        SidebarItem::Directory { depth, .. } | SidebarItem::File { depth, .. } => *depth,
    }
}

fn has_later_sibling_at_depth(
    visible_indices: &[usize],
    visible_pos: usize,
    items: &[SidebarItem],
    depth: usize,
) -> bool {
    for idx in visible_indices.iter().skip(visible_pos + 1) {
        let next_depth = item_depth(&items[*idx]);
        if next_depth < depth {
            return false;
        }
        if next_depth == depth {
            return true;
        }
    }
    false
}

fn tree_prefix(visible_indices: &[usize], visible_pos: usize, items: &[SidebarItem]) -> String {
    let depth = item_depth(&items[visible_indices[visible_pos]]);
    let mut prefix = String::new();

    for level in 0..depth {
        if has_later_sibling_at_depth(visible_indices, visible_pos, items, level) {
            prefix.push_str("│  ");
        } else {
            prefix.push_str("   ");
        }
    }

    if has_later_sibling_at_depth(visible_indices, visible_pos, items, depth) {
        prefix.push_str("├─");
    } else {
        prefix.push_str("└─");
    }

    prefix
}

fn collect_directory_stats(
    sidebar_items: &[SidebarItem],
    viewed_files: &HashSet<usize>,
) -> HashMap<String, DirectoryStats> {
    let mut stats = HashMap::new();

    for item in sidebar_items {
        if let SidebarItem::Directory { path, .. } = item {
            let mut dir_stats = DirectoryStats::default();
            let child_prefix = format!("{}/", path);

            for child in sidebar_items {
                if let SidebarItem::File {
                    path: file_path,
                    file_index,
                    status,
                    ..
                } = child
                {
                    if file_path.starts_with(&child_prefix) {
                        dir_stats.files += 1;
                        if viewed_files.contains(file_index) {
                            dir_stats.viewed += 1;
                        }
                        match status {
                            FileStatus::Added => dir_stats.added += 1,
                            FileStatus::Modified => dir_stats.modified += 1,
                            FileStatus::Deleted => dir_stats.deleted += 1,
                        }
                    }
                }
            }

            stats.insert(path.clone(), dir_stats);
        }
    }

    stats
}

#[allow(clippy::too_many_arguments)]
pub fn render_sidebar(
    frame: &mut Frame,
    area: Rect,
    sidebar_items: &[SidebarItem],
    sidebar_visible: &[usize],
    collapsed_dirs: &HashSet<String>,
    current_file: usize,
    sidebar_selected: usize,
    sidebar_scroll: usize,
    sidebar_h_scroll: u16,
    viewed_files: &HashSet<usize>,
    is_focused: bool,
    total_files: usize,
    total_added: usize,
    total_removed: usize,
) {
    let t = theme::get();
    let bg = t.ui.bg;
    let visible_height = area.height.saturating_sub(2) as usize;
    let directory_stats = collect_directory_stats(sidebar_items, viewed_files);
    let viewed_count = viewed_files.len().min(total_files);
    let lines: Vec<Line> = sidebar_visible
        .iter()
        .enumerate()
        .map(|(i, item_idx)| {
            let item = &sidebar_items[*item_idx];
            let is_selected = i == sidebar_selected;
            let base_style = if is_selected {
                Style::default().fg(t.ui.selection_fg).bg(if is_focused {
                    t.ui.selection_bg
                } else {
                    t.ui.border_unfocused
                })
            } else {
                Style::default()
            };

            let prefix = tree_prefix(sidebar_visible, i, sidebar_items);
            let prefix_style = if is_selected {
                base_style
            } else {
                Style::default().fg(t.ui.text_muted)
            };

            match item {
                SidebarItem::Directory { name, path, .. } => {
                    let stats = directory_stats.get(path);
                    let files = stats.map(|s| s.files).unwrap_or(0);
                    let viewed = stats.map(|s| s.viewed).unwrap_or(0);
                    let fully_viewed = files > 0 && viewed == files;
                    let arrow = if files > 0 {
                        if collapsed_dirs.contains(path) {
                            "▸"
                        } else {
                            "▾"
                        }
                    } else {
                        " "
                    };
                    let marker = if fully_viewed { "✓" } else { " " };
                    let dir_style = if is_selected {
                        base_style.add_modifier(Modifier::BOLD)
                    } else if fully_viewed {
                        Style::default()
                            .fg(t.ui.viewed)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(t.ui.text_primary)
                            .add_modifier(Modifier::BOLD)
                    };
                    let meta_style = if is_selected {
                        base_style
                    } else {
                        Style::default().fg(t.ui.text_muted)
                    };

                    let mut spans = vec![
                        Span::styled(prefix, prefix_style),
                        Span::styled(marker, base_style),
                        Span::styled(" ", base_style),
                        Span::styled(arrow, dir_style),
                        Span::styled(" ", base_style),
                        Span::styled(format!("{}/", name), dir_style),
                    ];

                    if files > 0 {
                        spans.push(Span::styled(format!("  {}/{}", viewed, files), meta_style));
                    }

                    if let Some(stats) = stats {
                        if stats.added > 0 {
                            spans.push(Span::styled(
                                format!(" +{}", stats.added),
                                if is_selected {
                                    base_style
                                } else {
                                    Style::default().fg(t.ui.status_added)
                                },
                            ));
                        }
                        if stats.modified > 0 {
                            spans.push(Span::styled(
                                format!(" ~{}", stats.modified),
                                if is_selected {
                                    base_style
                                } else {
                                    Style::default().fg(t.ui.status_modified)
                                },
                            ));
                        }
                        if stats.deleted > 0 {
                            spans.push(Span::styled(
                                format!(" -{}", stats.deleted),
                                if is_selected {
                                    base_style
                                } else {
                                    Style::default().fg(t.ui.status_deleted)
                                },
                            ));
                        }
                    }

                    Line::from(spans)
                }
                SidebarItem::File {
                    name,
                    file_index,
                    status,
                    ..
                } => {
                    let viewed = viewed_files.contains(file_index);
                    let is_current = *file_index == current_file;
                    let marker = if is_current {
                        "●"
                    } else if viewed {
                        "✓"
                    } else {
                        "·"
                    };
                    let status_color = match status {
                        FileStatus::Modified => t.ui.status_modified,
                        FileStatus::Added => t.ui.status_added,
                        FileStatus::Deleted => t.ui.status_deleted,
                    };
                    let status_style = if is_selected {
                        base_style.add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(status_color)
                            .add_modifier(Modifier::BOLD)
                    };
                    let name_style = if is_selected {
                        base_style
                    } else if is_current {
                        Style::default()
                            .fg(t.ui.highlight)
                            .add_modifier(Modifier::BOLD)
                    } else if viewed {
                        Style::default().fg(t.ui.viewed)
                    } else {
                        Style::default().fg(t.ui.text_secondary)
                    };
                    let marker_style = if is_selected {
                        base_style
                    } else if is_current {
                        Style::default().fg(t.ui.highlight)
                    } else if viewed {
                        Style::default().fg(t.ui.viewed)
                    } else {
                        Style::default().fg(t.ui.text_muted)
                    };

                    Line::from(vec![
                        Span::styled(prefix, prefix_style),
                        Span::styled(marker, marker_style),
                        Span::styled(" ", base_style),
                        Span::styled(status.symbol(), status_style),
                        Span::styled(" ", base_style),
                        Span::styled(name.clone(), name_style),
                    ])
                }
            }
        })
        .collect();

    let title_style = if is_focused {
        Style::default().fg(t.ui.border_focused)
    } else {
        Style::default().fg(t.ui.border_unfocused)
    };
    let border_style = if is_focused {
        Style::default().fg(t.ui.border_focused)
    } else {
        Style::default().fg(t.ui.border_unfocused)
    };
    let muted_style = Style::default().fg(t.ui.text_muted);

    let title = Line::from(vec![
        Span::styled(" [1] Review ", title_style.add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("{} / {} files ", viewed_count, total_files),
            muted_style,
        ),
        Span::styled(
            format!("+{}", total_added),
            Style::default().fg(t.ui.stats_added),
        ),
        Span::raw(" "),
        Span::styled(
            format!("-{}", total_removed),
            Style::default().fg(t.ui.stats_removed),
        ),
        Span::raw(" "),
    ]);

    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(sidebar_scroll)
        .take(visible_height)
        .collect();

    // Drop the right border so the sidebar shares a single vertical line with
    // the adjacent diff panel. The parent renderer fixes up the corner cells
    // at the boundary so the joined borders use `┬` / `┴` junctions.
    let borders = Borders::TOP | Borders::LEFT | Borders::BOTTOM;

    let para = Paragraph::new(visible_lines)
        .style(Style::default().bg(bg))
        .scroll((0, sidebar_h_scroll))
        .block(
            Block::default()
                .title(title)
                .borders(borders)
                .border_style(border_style)
                .style(Style::default().bg(bg)),
        );

    frame.render_widget(para, area);
}
