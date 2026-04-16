use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, layout::Rect};

use crate::config::AnimationMode;
use crate::tui::state::config_state::GeneralConfigForm;

pub fn render(frame: &mut Frame, area: Rect, form: &GeneralConfigForm) {
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

    let title = Paragraph::new("常规").style(Style::default().fg(Color::Rgb(86, 95, 137)).bold());
    frame.render_widget(title, chunks[0]);

    let lang = format!("语言                [ {} \u{25bc} ]", form.language);
    frame.render_widget(
        Paragraph::new(lang).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[1],
    );

    let vault_display = form.vault_path.display();
    let vault = format!("Vault 路径          {}  [ 修改 ]", vault_display);
    frame.render_widget(
        Paragraph::new(vault).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[2],
    );

    let auto_lock = format!("自动锁定          [ {}秒 \u{25bc} ]", form.auto_lock_seconds);
    frame.render_widget(
        Paragraph::new(auto_lock).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[3],
    );

    let clip = format!(
        "剪贴板清除        [ {}秒 \u{25bc} ]",
        form.clipboard_clear_seconds
    );
    frame.render_widget(
        Paragraph::new(clip).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[4],
    );

    let trash = format!(
        "回收站保留天数    [ {}天 \u{25bc} ]",
        form.trash_retention_days
    );
    frame.render_widget(
        Paragraph::new(trash).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[5],
    );

    let anim_label = match form.animation {
        AnimationMode::Auto => "自动",
        AnimationMode::On => "开启",
        AnimationMode::Off => "关闭",
    };
    let anim = format!("动画效果          [ {} \u{25bc} ]", anim_label);
    frame.render_widget(
        Paragraph::new(anim).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[6],
    );

    let btns = "[ 从其他管理器导入... ]  [ 导出数据... ]";
    frame.render_widget(
        Paragraph::new(btns).style(Style::default().fg(Color::Rgb(122, 162, 247))),
        chunks[7],
    );
}
