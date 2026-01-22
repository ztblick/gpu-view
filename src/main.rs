use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nvml_wrapper::Nvml;
use rand::Rng;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame, Terminal,
};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};

struct GpuData {
    name: String,
    memory_used: u64,
    memory_total: u64,
    utilization: u32,
    temperature: u32,
    fan_speed: u32,
    power_draw: u32,     // milliwatts
    power_limit: u32,    // milliwatts
    clock_speed: u32,    // MHz
    sm_count: u32,
}

impl GpuData {
    fn memory_percent(&self) -> f64 {
        if self.memory_total == 0 {
            return 0.0;
        }
        (self.memory_used as f64 / self.memory_total as f64) * 100.0
    }
}

fn fetch_gpu_data(nvml: &Nvml) -> Result<GpuData> {
    let device = nvml.device_by_index(0).context("Failed to get GPU device")?;

    let name = device.name().unwrap_or_else(|_| "Unknown GPU".to_string());

    let memory_info = device.memory_info().context("Failed to get memory info")?;

    let utilization = device
        .utilization_rates()
        .map(|u| u.gpu)
        .unwrap_or(0);

    let temperature = device
        .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
        .unwrap_or(0);

    let fan_speed = device.fan_speed(0).unwrap_or(0);

    let power_draw = device.power_usage().unwrap_or(0);
    let power_limit = device.power_management_limit().unwrap_or(0);

    let clock_speed = device
        .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Graphics)
        .unwrap_or(0);

    Ok(GpuData {
        name,
        memory_used: memory_info.used,
        memory_total: memory_info.total,
        utilization,
        temperature,
        fan_speed,
        power_draw,
        power_limit,
        clock_speed,
        sm_count: 24, // Fixed at 24 for clean 8x3 grid display
    })
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn render_memory_bar(frame: &mut Frame, area: Rect, gpu: &GpuData) {
    let percent = gpu.memory_percent();
    let used_gb = gpu.memory_used as f64 / 1_073_741_824.0;
    let total_gb = gpu.memory_total as f64 / 1_073_741_824.0;

    let color = if percent > 90.0 {
        Color::Red
    } else if percent > 70.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Memory"))
        .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
        .percent(percent as u16)
        .label(format!("{:.1} / {:.1} GB ({:.0}%)", used_gb, total_gb, percent));

    frame.render_widget(gauge, area);
}

fn render_utilization_bar(frame: &mut Frame, area: Rect, gpu: &GpuData) {
    let color = if gpu.utilization > 90 {
        Color::Red
    } else if gpu.utilization > 70 {
        Color::Yellow
    } else {
        Color::Green
    };

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("GPU Utilization"))
        .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
        .percent(gpu.utilization as u16)
        .label(format!("{}%", gpu.utilization));

    frame.render_widget(gauge, area);
}

fn temp_color(temp_f: u32) -> Color {
    if temp_f >= 185 {
        Color::Red        // Danger/shutdown
    } else if temp_f >= 158 {
        Color::Yellow     // Concern
    } else if temp_f >= 120 {
        Color::Green      // Acceptable
    } else {
        Color::Cyan       // Cold/good
    }
}

fn render_thermometer(frame: &mut Frame, area: Rect, gpu: &GpuData) {
    let temp_c = gpu.temperature;
    let temp_f = (temp_c as f32 * 9.0 / 5.0 + 32.0) as u32;

    // Temperature range: 100°F to 200°F
    let min_temp_f: u32 = 100;
    let max_temp_f: u32 = 200;

    let color = temp_color(temp_f);

    let inner = Block::default()
        .borders(Borders::ALL)
        .title("Temperature")
        .style(Style::default());

    let inner_area = inner.inner(area);
    frame.render_widget(inner, area);

    if inner_area.height < 6 {
        return;
    }

    // Fixed layout: 6 temperature labels (200, 180, 160, 140, 120, 100)
    let bulb_height: u16 = 3;
    let reading_height: u16 = 1;
    let tube_height = inner_area.height.saturating_sub(bulb_height + reading_height + 1);

    // Calculate fill level based on 100-200°F range
    // Minimum fill of 1 so it always pokes into the tube body
    let temp_clamped = temp_f.clamp(min_temp_f, max_temp_f);
    let fill_ratio = (temp_clamped - min_temp_f) as f32 / (max_temp_f - min_temp_f) as f32;
    let fill_level = ((fill_ratio * (tube_height - 1) as f32) as u16) + 1; // Always at least 1

    let mut lines: Vec<Line> = Vec::new();

    // Top of tube with 200°F / 93°C labels (red)
    lines.push(Line::from(vec![
        Span::styled("200°F", Style::default().fg(Color::Red)),
        Span::styled("─", Style::default().fg(Color::Red)),
        Span::styled("╭───╮", Style::default().fg(Color::White)),
        Span::styled("─", Style::default().fg(Color::Red)),
        Span::styled("93°C", Style::default().fg(Color::Red)),
    ]));

    // Label temperatures with their Celsius equivalents
    let label_temps: Vec<(u32, u32)> = vec![(180, 82), (160, 71), (140, 60), (120, 49), (100, 38)];

    // Pre-calculate which row each label should appear on
    let mut label_rows: std::collections::HashMap<u16, (u32, u32)> = std::collections::HashMap::new();
    for &(temp_f_label, temp_c_label) in &label_temps {
        let label_ratio = (max_temp_f - temp_f_label) as f32 / (max_temp_f - min_temp_f) as f32;
        let mut row = (label_ratio * (tube_height - 1) as f32).round() as u16;
        // Move 160 and 180 up one line for better spacing
        if temp_f_label == 160 || temp_f_label == 180 {
            row = row.saturating_sub(1);
        }
        label_rows.insert(row, (temp_f_label, temp_c_label));
    }

    // Tube body
    for i in 0..tube_height {
        let from_bottom = tube_height - 1 - i;
        let filled = from_bottom < fill_level;

        // Calculate temperature at this row for fill color
        let row_ratio = 1.0 - (i as f32 / tube_height as f32);
        let row_temp_f = min_temp_f as f32 + row_ratio * (max_temp_f - min_temp_f) as f32;
        let row_temp_f_int = row_temp_f as u32;

        // Color the fill based on the temperature at each level
        let fill_color = if filled {
            temp_color(row_temp_f_int)
        } else {
            Color::DarkGray
        };
        let fill_char = if filled { " █ " } else { "   " };

        // Check if this row has a label
        let (axis_label_f, axis_label_c, label_color, tick, tick_color) =
            if let Some(&(temp_f_label, temp_c_label)) = label_rows.get(&(i as u16)) {
                let lbl_color = if temp_f_label >= 180 {
                    Color::Yellow
                } else {
                    Color::DarkGray
                };
                (format!("{:>3}°F", temp_f_label), format!("{}°C", temp_c_label), lbl_color, "─", lbl_color)
            } else {
                ("     ".to_string(), "    ".to_string(), Color::DarkGray, " ", Color::DarkGray)
            };

        lines.push(Line::from(vec![
            Span::styled(axis_label_f, Style::default().fg(label_color)),
            Span::styled(tick, Style::default().fg(tick_color)),
            Span::styled("│", Style::default().fg(Color::White)),
            Span::styled(fill_char, Style::default().fg(fill_color)),
            Span::styled("│", Style::default().fg(Color::White)),
            Span::styled(tick, Style::default().fg(tick_color)),
            Span::styled(axis_label_c, Style::default().fg(label_color)),
        ]));
    }

    // Bulb (connected to tube body, always fully filled)
    // Transition row connects narrow tube to wider bulb
    lines.push(Line::from(vec![
        Span::raw("     "),
        Span::styled("╭┘", Style::default().fg(Color::White)),
        Span::styled(" █ ", Style::default().fg(color)),
        Span::styled("└╮", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("     "),
        Span::styled("│", Style::default().fg(Color::White)),
        Span::styled(" ███ ", Style::default().fg(color)),
        Span::styled("│", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("     "),
        Span::styled("╰─────╯", Style::default().fg(Color::White)),
    ]));

    // Temperature reading (both C and F)
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{}°C / {}°F", temp_c, temp_f),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner_area);
}

fn render_fan_speed_bar(frame: &mut Frame, area: Rect, gpu: &GpuData) {
    let fan = gpu.fan_speed;

    let color = if fan > 80 {
        Color::Red
    } else if fan > 50 {
        Color::Yellow
    } else {
        Color::Cyan
    };

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Fan Speed"))
        .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
        .percent(fan as u16)
        .label(format!("{}%", fan));

    frame.render_widget(gauge, area);
}

fn render_power_clock(frame: &mut Frame, area: Rect, gpu: &GpuData) {
    let power_w = gpu.power_draw as f64 / 1000.0;
    let limit_w = gpu.power_limit as f64 / 1000.0;
    let power_percent = if limit_w > 0.0 {
        (power_w / limit_w * 100.0) as u32
    } else {
        0
    };

    let power_color = if power_percent > 90 {
        Color::Red
    } else if power_percent > 70 {
        Color::Yellow
    } else {
        Color::Green
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Power / Clock");

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled(" ⚡ Power: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{:.1}W / {:.1}W", power_w, limit_w),
                Style::default().fg(power_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(" ⏱  Clock: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{} MHz", gpu.clock_speed),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner_area);
}

fn render_sm_grid(frame: &mut Frame, area: Rect, gpu: &GpuData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Streaming Multiprocessors");

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if inner_area.height < 5 || inner_area.width < 16 {
        return;
    }

    let sm_count = gpu.sm_count as usize;
    let utilization = gpu.utilization as f32 / 100.0;
    let active_sms = (sm_count as f32 * utilization).round() as usize;

    // Randomly select which SMs are "active"
    let mut rng = rand::thread_rng();
    let mut sm_states: Vec<bool> = vec![false; sm_count];
    let mut activated = 0;
    while activated < active_sms && activated < sm_count {
        let idx = rng.gen_range(0..sm_count);
        if !sm_states[idx] {
            sm_states[idx] = true;
            activated += 1;
        }
    }

    // Fixed grid: 8 columns x 3 rows = 24 SMs
    let cols = 8;
    let rows = 3;

    // Calculate dynamic cell width with padding
    let padding = 4; // 2 chars on each side
    let available_width = inner_area.width as usize - padding;
    let cell_width = available_width / cols;
    let cell_inner = cell_width.saturating_sub(2); // Subtract 2 for left/right borders
    let left_padding = (inner_area.width as usize - (cell_width * cols)) / 2;

    let mut lines: Vec<Line> = Vec::new();

    for row in 0..rows {
        // Top border of cells: ┌───┐
        let mut top_spans: Vec<Span> = Vec::new();
        top_spans.push(Span::raw(" ".repeat(left_padding)));
        for col in 0..cols {
            let idx = row * cols + col;
            if idx < sm_count {
                let border_color = if sm_states[idx] { Color::Green } else { Color::DarkGray };
                let top_border = format!("┌{}┐", "─".repeat(cell_inner));
                top_spans.push(Span::styled(top_border, Style::default().fg(border_color)));
            }
        }
        lines.push(Line::from(top_spans));

        // Middle of cells: │██│ or │  │
        let mut mid_spans: Vec<Span> = Vec::new();
        mid_spans.push(Span::raw(" ".repeat(left_padding)));
        for col in 0..cols {
            let idx = row * cols + col;
            if idx < sm_count {
                let border_color = if sm_states[idx] { Color::Green } else { Color::DarkGray };
                if sm_states[idx] {
                    let fill = "█".repeat(cell_inner);
                    mid_spans.push(Span::styled(format!("│{}│", fill), Style::default().fg(Color::Green)));
                } else {
                    let empty = " ".repeat(cell_inner);
                    mid_spans.push(Span::styled(format!("│{}│", empty), Style::default().fg(border_color)));
                }
            }
        }
        lines.push(Line::from(mid_spans));

        // Bottom border of cells: └───┘
        let mut bot_spans: Vec<Span> = Vec::new();
        bot_spans.push(Span::raw(" ".repeat(left_padding)));
        for col in 0..cols {
            let idx = row * cols + col;
            if idx < sm_count {
                let border_color = if sm_states[idx] { Color::Green } else { Color::DarkGray };
                let bot_border = format!("└{}┘", "─".repeat(cell_inner));
                bot_spans.push(Span::styled(bot_border, Style::default().fg(border_color)));
            }
        }
        lines.push(Line::from(bot_spans));
    }

    // Add utilization label
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(left_padding)),
        Span::styled(
            format!("{} SMs @ {}% utilization", sm_count, gpu.utilization),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner_area);
}

fn render_header(frame: &mut Frame, area: Rect, gpu: &GpuData) {
    let width = area.width as usize;
    let name = &gpu.name;
    let name_with_spaces = format!(" {} ", name);
    let name_len = name_with_spaces.len();

    // Calculate line lengths on each side
    let remaining = width.saturating_sub(name_len);
    let left_len = remaining / 2;
    let right_len = remaining - left_len;

    let title_line = format!(
        "{}{}{}",
        "═".repeat(left_len),
        name_with_spaces,
        "═".repeat(right_len)
    );

    let lines = vec![
        Line::from(""), // Padding line above
        Line::from(vec![
            Span::styled(
                title_line,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let header = Paragraph::new(lines);
    frame.render_widget(header, area);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            " Press 'q' then Enter to quit ",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    frame.render_widget(footer, area);
}

fn ui(frame: &mut Frame, gpu: &GpuData) {
    let terminal_size = frame.size();

    // Use up to half the terminal height, minimum 30 rows for all content
    // Layout needs: header(2) + footer(1) + memory(3) + util(3) + fan(3) + power(4) + SM(12) = 28 min
    let min_height = 30;
    let target_height = (terminal_size.height / 2).max(min_height);
    let display_height = target_height.min(terminal_size.height);

    // Create display area at top of terminal
    let display_area = Rect {
        x: terminal_size.x,
        y: terminal_size.y,
        width: terminal_size.width,
        height: display_height,
    };

    // Main layout: header, content, footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Header
            Constraint::Min(20),    // Content
            Constraint::Length(1),  // Footer
        ])
        .split(display_area);

    render_header(frame, main_chunks[0], gpu);
    render_footer(frame, main_chunks[2]);

    // Content area: left (3/4) and right (1/4) columns
    let content_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(main_chunks[1]);

    // Left column: metrics at top, SM grid below
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Memory
            Constraint::Length(3),  // Utilization
            Constraint::Length(3),  // Fan Speed
            Constraint::Length(4),  // Power/Clock
            Constraint::Min(10),    // SM grid
        ])
        .split(content_columns[0]);

    render_memory_bar(frame, left_chunks[0], gpu);
    render_utilization_bar(frame, left_chunks[1], gpu);
    render_fan_speed_bar(frame, left_chunks[2], gpu);
    render_power_clock(frame, left_chunks[3], gpu);
    render_sm_grid(frame, left_chunks[4], gpu);

    // Right column: Thermometer (full height to bottom)
    render_thermometer(frame, content_columns[1], gpu);
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, nvml: &Nvml) -> Result<()> {
    let tick_rate = Duration::from_secs(1);
    let mut last_tick = Instant::now();
    let mut input_buffer = String::new();

    loop {
        let gpu_data = fetch_gpu_data(nvml)?;

        terminal.draw(|frame| ui(frame, &gpu_data))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char(c) => {
                            input_buffer.push(c);
                        }
                        KeyCode::Enter => {
                            if input_buffer.trim() == "q" {
                                return Ok(());
                            }
                            input_buffer.clear();
                        }
                        KeyCode::Backspace => {
                            input_buffer.pop();
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

fn main() -> Result<()> {
    // Initialize NVML
    let nvml = match Nvml::init() {
        Ok(nvml) => nvml,
        Err(e) => {
            eprintln!("╔════════════════════════════════════════════════════════╗");
            eprintln!("║  Error: Could not initialize NVIDIA GPU monitoring     ║");
            eprintln!("╠════════════════════════════════════════════════════════╣");
            eprintln!("║  {}",  format!("{:<55}║", format!("Details: {}", e)));
            eprintln!("║                                                        ║");
            eprintln!("║  Please ensure:                                        ║");
            eprintln!("║  • An NVIDIA GPU is installed                          ║");
            eprintln!("║  • NVIDIA drivers are properly installed               ║");
            eprintln!("║  • The nvidia-smi command works                        ║");
            eprintln!("╚════════════════════════════════════════════════════════╝");
            std::process::exit(1);
        }
    };

    // Verify we have at least one GPU
    let device_count = nvml.device_count().context("Failed to get device count")?;
    if device_count == 0 {
        eprintln!("Error: No NVIDIA GPUs detected.");
        std::process::exit(1);
    }

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Run the application
    let result = run_app(&mut terminal, &nvml);

    // Restore terminal
    restore_terminal(&mut terminal)?;

    result
}
