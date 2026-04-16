use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, layout::Rect};

use crate::config::AnimationMode;
use crate::tui::state::config_state::GeneralConfigForm;
use crate::tui::theme;

pub fn render(frame: &mut Frame, area: Rect, form: &GeneralConfigForm, focused: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Language
            Constraint::Length(1), // Vault path
            Constraint::Length(1), // Auto lock
            Constraint::Length(1), // Clipboard
            Constraint::Length(1), // Trash
            Constraint::Length(1), // Animation
            Constraint::Length(1), // Import/Export buttons
        ])
        .split(area);

    let dim_style = Style::default().fg(theme::TEXT_SECONDARY).bold();
    let normal_style = Style::default().fg(theme::TEXT);
    let focused_style = Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::BOLD)
        .bg(theme::BG_SURFACE);
    let accent_style = Style::default().fg(theme::PRIMARY);

    // Title row is NOT focusable — always dim
    let title = Paragraph::new("常规").style(dim_style);
    frame.render_widget(title, chunks[0]);

    // Row index 0: Language (focused == 0)
    let lang = format!("语言                [ {} \u{25bc} ]", form.language);
    frame.render_widget(
        Paragraph::new(lang).style(if focused == 0 {
            focused_style
        } else {
            normal_style
        }),
        chunks[1],
    );

    // Row index 1: Vault path (focused == 1)
    let vault_display = form.vault_path.display();
    let vault = format!("Vault 路径          {}  [ 修改 ]", vault_display);
    frame.render_widget(
        Paragraph::new(vault).style(if focused == 1 {
            focused_style
        } else {
            normal_style
        }),
        chunks[2],
    );

    // Row index 2: Auto lock (focused == 2)
    let auto_lock = format!("自动锁定          [ {}秒 \u{25bc} ]", form.auto_lock_seconds);
    frame.render_widget(
        Paragraph::new(auto_lock).style(if focused == 2 {
            focused_style
        } else {
            normal_style
        }),
        chunks[3],
    );

    // Row index 3: Clipboard (focused == 3)
    let clip = format!(
        "剪贴板清除        [ {}秒 \u{25bc} ]",
        form.clipboard_clear_seconds
    );
    frame.render_widget(
        Paragraph::new(clip).style(if focused == 3 {
            focused_style
        } else {
            normal_style
        }),
        chunks[4],
    );

    // Row index 4: Trash (focused == 4)
    let trash = format!(
        "回收站保留天数    [ {}天 \u{25bc} ]",
        form.trash_retention_days
    );
    frame.render_widget(
        Paragraph::new(trash).style(if focused == 4 {
            focused_style
        } else {
            normal_style
        }),
        chunks[5],
    );

    // Row index 5: Animation (focused == 5)
    let anim_label = match form.animation {
        AnimationMode::Auto => "自动",
        AnimationMode::On => "开启",
        AnimationMode::Off => "关闭",
    };
    let anim = format!("动画效果          [ {} \u{25bc} ]", anim_label);
    frame.render_widget(
        Paragraph::new(anim).style(if focused == 5 {
            focused_style
        } else {
            normal_style
        }),
        chunks[6],
    );

    // Row index 6: Import/Export buttons (focused == 6)
    let btns = "[ 从其他管理器导入... ]  [ 导出数据... ]";
    frame.render_widget(
        Paragraph::new(btns).style(if focused == 6 {
            focused_style
        } else {
            accent_style
        }),
        chunks[7],
    );
}
