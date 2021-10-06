use anyhow::{anyhow, Result};
use async_std::prelude::*;
use signal_hook::consts::signal::*;
use signal_hook_async_std::Signals;
use std::process::exit;
use structopt::StructOpt;
use swayipc_async::{ Connection, Event, EventType, NodeLayout, NodeType, WindowChange, Workspace, WorkspaceChange };

#[derive(StructOpt)]
/// Wayout daemon.
///
/// I talk to the Sway Compositor and persuade it to do little evil things.
/// Give me an option and see what it brings.
struct Cli {
    /// Called when persway exits. This can be used to reset any opacity changes
    /// or other settings when persway exits. For example, if changing the opacity
    /// on window focus, you would probably want to reset that on exit like this:
    ///
    /// [tiling] opacity 1
    ///
    /// Eg. set all tiling windows to opacity 1
    #[structopt(short = "e", long = "on-exit")]
    on_exit: Option<String>,
}

async fn handle_signals(signals: Signals) {
    let mut signals = signals.fuse();
    let args = Cli::from_args();
    let on_exit = args.on_exit.unwrap_or_else(|| String::from(""));
    while let Some(signal) = signals.next().await {
        match signal {
            SIGHUP | SIGINT | SIGQUIT | SIGTERM => {
                let mut commands = Connection::new().await.unwrap();
                commands.run_command(format!("{}", on_exit)).await.unwrap();
                exit(0)
            }
            _ => unreachable!(),
        }
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let signals = Signals::new(&[SIGHUP, SIGINT, SIGQUIT, SIGTERM])?;
    let handle = signals.handle();
    let signals_task = async_std::task::spawn(handle_signals(signals));

    let mut conection = Connection::new().await?;
    let subs = [EventType::Window, EventType::Workspace];

    let mut events = Connection::new().await?.subscribe(&subs).await?;
    while let Some(event) = events.next().await {
        match event? {
            Event::Window(event) => match event.change {
                WindowChange::New => {
                    autolayout(&mut conection).await?
                }
                WindowChange::Close => {
                    autolayout(&mut conection).await?
                }
                _ => {}
            },
            Event::Workspace(event) => match event.change {
                WorkspaceChange::Init => {
                    conection.run_command("gaps horizontal current set 752").await?;
                }
                _ => {}
            },
            _ => unreachable!(),
        }
    }

    handle.close();
    signals_task.await;
    Ok(())
}

async fn autolayout(conn: &mut Connection) -> Result<()> {
    let tree = conn.get_tree().await?;

    let focused = tree
        .find_focused_as_ref(|n| n.focused)
        .ok_or(anyhow!("No focused node"))?;

    if focused.node_type == NodeType::FloatingCon || focused.percent.unwrap_or(1.0) > 1.0 {
        return Ok(());
    }

    let parent = tree
        .find_focused_as_ref(|n| n.nodes.iter().any(|c| c.focused))
        .ok_or(anyhow!("No parent"))?;

    if parent.layout == NodeLayout::Stacked || parent.layout == NodeLayout::Tabbed {
        return Ok(());
    }

    // Not the first Node
    if parent.node_type != NodeType::Workspace || parent.nodes.len() > 1 {
        conn.run_command(format!("gaps right current set 0")).await?;
        // conn.run_command(format!("split v")).await?;

        let workspace = get_focused_workspace(conn).await?;
        conn.run_command(format!("[con_mark=\"main_{}\"] resize set 1920px", workspace.id)).await?;

        return Ok(());
    }

    conn.run_command("gaps horizontal current set 752").await?;

    conn.run_command(format!("mark --add main_{}", parent.id)).await?;

    Ok(())
}

async fn get_focused_workspace(conn: &mut Connection) -> Result<Workspace> {
    let mut ws = conn.get_workspaces().await?.into_iter();
    ws.find(|w| w.focused)
        .ok_or(anyhow!("No focused workspace"))
}
