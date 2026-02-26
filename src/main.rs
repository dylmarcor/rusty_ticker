use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::{backend::CrosstermBackend, Terminal};
use reqwest::Client;
use serde::Deserialize;

const BINANCE_TICKER_24H_URL: &str = "https://api.binance.com/api/v3/ticker/24hr";

#[derive(Debug, Parser)]
#[command(name = "rusty_ticker", about = "Live market ticker in your terminal")]
struct Args {
    /// Comma-separated list of symbols to track (ex: BTCUSDT,ETHUSDT,SOLUSDT)
    #[arg(short, long, default_value = "BTCUSDT,ETHUSDT,SOLUSDT")]
    symbols: String,

    /// Update interval in seconds
    #[arg(short, long, default_value_t = 2)]
    interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceTicker {
    symbol: String,
    last_price: String,
    price_change_percent: String,
    high_price: String,
    low_price: String,
    volume: String,
}

#[derive(Debug)]
struct TickerRow {
    symbol: String,
    last_price: f64,
    change_pct: f64,
    high: f64,
    low: f64,
    volume: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let symbols: Vec<String> = args
        .symbols
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();

    if symbols.is_empty() {
        anyhow::bail!("No symbols were provided. Use --symbols BTCUSDT,ETHUSDT");
    }

    let app = App::new(symbols, Duration::from_secs(args.interval));
    app.run().await
}

struct App {
    client: Client,
    symbols: Vec<String>,
    interval: Duration,
    last_updated: Option<Instant>,
    rows: Vec<TickerRow>,
    status: String,
}

impl App {
    fn new(symbols: Vec<String>, interval: Duration) -> Self {
        Self {
            client: Client::new(),
            symbols,
            interval,
            last_updated: None,
            rows: Vec::new(),
            status: "Starting…".to_string(),
        }
    }

    async fn run(mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_loop(&mut terminal).await;

        disable_raw_mode()?;
        io::stdout().execute(LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    async fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        self.refresh().await;

        loop {
            terminal.draw(|f| self.draw(f))?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    let quit = key.code == KeyCode::Char('q')
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL));
                    if quit {
                        break;
                    }
                }
            }

            let stale = self
                .last_updated
                .map(|t| t.elapsed() >= self.interval)
                .unwrap_or(true);
            if stale {
                self.refresh().await;
            }
        }

        Ok(())
    }

    async fn refresh(&mut self) {
        match fetch_tickers(&self.client, &self.symbols).await {
            Ok(rows) => {
                self.rows = rows;
                self.last_updated = Some(Instant::now());
                self.status = "OK".to_string();
            }
            Err(error) => {
                self.status = format!("Fetch error: {error}");
            }
        }
    }

    fn draw(&self, frame: &mut ratatui::Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(2),
            ])
            .split(frame.size());

        let header = Paragraph::new("rusty_ticker — q to quit")
            .block(Block::default().borders(Borders::ALL).title("Market Feed"));
        frame.render_widget(header, chunks[0]);

        let table = Table::new(
            self.rows.iter().map(|row| {
                let change_color = if row.change_pct >= 0.0 {
                    Color::Green
                } else {
                    Color::Red
                };

                Row::new(vec![
                    Cell::from(row.symbol.clone()),
                    Cell::from(format!("{:.4}", row.last_price)),
                    Cell::from(format!("{:+.2}%", row.change_pct))
                        .style(Style::default().fg(change_color)),
                    Cell::from(format!("{:.4}", row.high)),
                    Cell::from(format!("{:.4}", row.low)),
                    Cell::from(format!("{:.2}", row.volume)),
                ])
            }),
            [
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Length(14),
                Constraint::Length(14),
            ],
        )
        .header(
            Row::new(vec![
                "Symbol", "Last", "24h %", "24h High", "24h Low", "Volume",
            ])
            .style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            ),
        )
        .block(Block::default().borders(Borders::ALL).title("Tickers"));
        frame.render_widget(table, chunks[1]);

        let updated = self
            .last_updated
            .map(|i| format!("Updated {}s ago", i.elapsed().as_secs()))
            .unwrap_or_else(|| "Never updated".to_string());

        let footer = Paragraph::new(format!("Status: {} | {}", self.status, updated))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }
}

async fn fetch_tickers(client: &Client, symbols: &[String]) -> Result<Vec<TickerRow>> {
    let mut rows = Vec::new();

    for symbol in symbols {
        let data = client
            .get(BINANCE_TICKER_24H_URL)
            .query(&[("symbol", symbol)])
            .send()
            .await?
            .error_for_status()?
            .json::<BinanceTicker>()
            .await?;

        rows.push(TickerRow {
            symbol: data.symbol,
            last_price: parse_num(&data.last_price)?,
            change_pct: parse_num(&data.price_change_percent)?,
            high: parse_num(&data.high_price)?,
            low: parse_num(&data.low_price)?,
            volume: parse_num(&data.volume)?,
        });
    }

    Ok(rows)
}

fn parse_num(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .map_err(|e| anyhow::anyhow!("failed to parse '{value}' as number: {e}"))
}
