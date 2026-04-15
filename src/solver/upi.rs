//! UPI (Universal Poker Interface) client for communicating with PIOSolver.
//!
//! PIOSolver's free version supports UPI over stdin/stdout when launched as a
//! subprocess. This module implements the protocol for:
//! - Setting up game trees (board, ranges, bet sizes, stacks)
//! - Running the solver for N iterations
//! - Querying strategies at specific nodes
//! - Comparing strategies against our solver's output
//!
//! # Usage
//!
//! Requires PIOSolver to be installed. Set the `PIOSOLVER_PATH` environment
//! variable to the PIOSolver executable path.
//!
//! ```ignore
//! let mut client = UpiClient::launch(Path::new("/path/to/PioSOLVER2-free"))?;
//! client.set_board("As Kh Qd 7c 2s")?;
//! client.set_range(0, "AA,KK,QQ,AKs")?;
//! client.set_range(1, "JJ,TT,AQs,AJs")?;
//! client.set_pot(100.0, 100.0, 200.0)?;
//! client.build_tree()?;
//! client.solve(1000)?;
//! let strategy = client.get_strategy("r:0")?;
//! ```

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::{bail, Context, Result};

/// UPI client for communicating with PIOSolver via stdin/stdout.
pub struct UpiClient {
    child: Child,
    stdin: BufWriter<std::process::ChildStdin>,
    stdout: BufReader<std::process::ChildStdout>,
}

impl UpiClient {
    /// Launch PIOSolver as a subprocess and establish UPI communication.
    pub fn launch(piosolver_path: &Path) -> Result<Self> {
        let mut child = Command::new(piosolver_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("Failed to launch PIOSolver at {:?}", piosolver_path))?;

        let stdin = BufWriter::new(child.stdin.take().unwrap());
        let stdout = BufReader::new(child.stdout.take().unwrap());

        let mut client = Self {
            child,
            stdin,
            stdout,
        };

        // Wait for the initial ready signal
        client.wait_for_ready()?;

        Ok(client)
    }

    /// Send a command and return the response lines.
    pub fn send_command(&mut self, command: &str) -> Result<Vec<String>> {
        writeln!(self.stdin, "{}", command)?;
        self.stdin.flush()?;
        self.read_response()
    }

    /// Set the board cards (e.g., "As Kh Qd 7c 2s").
    pub fn set_board(&mut self, board: &str) -> Result<()> {
        self.send_command(&format!("set_board {}", board))?;
        Ok(())
    }

    /// Set a player's range (player 0 = OOP, player 1 = IP).
    pub fn set_range(&mut self, player: u8, range_str: &str) -> Result<()> {
        let side = if player == 0 { "OOP" } else { "IP" };
        self.send_command(&format!("set_range {} {}", side, range_str))?;
        Ok(())
    }

    /// Set the pot and stack sizes.
    /// `pot_oop` and `pot_ip` are each player's contribution, `stack` is the effective stack.
    pub fn set_pot(&mut self, pot_oop: f32, pot_ip: f32, stack: f32) -> Result<()> {
        self.send_command(&format!("set_pot {} {} {}", pot_oop, pot_ip, stack))?;
        Ok(())
    }

    /// Set bet sizes for a specific street and player.
    /// `sizes` is a comma-separated list of pot fractions (e.g., "50,100" for 50% and 100%).
    pub fn set_bet_sizes(&mut self, street: &str, player: &str, sizes: &str) -> Result<()> {
        self.send_command(&format!("set_bet_sizes {} {} {}", street, player, sizes))?;
        Ok(())
    }

    /// Build the game tree with current settings.
    pub fn build_tree(&mut self) -> Result<()> {
        self.send_command("build_tree")?;
        Ok(())
    }

    /// Run the solver for `iterations` iterations.
    pub fn solve(&mut self, iterations: u32) -> Result<()> {
        let response = self.send_command(&format!("go {} {}", iterations, iterations))?;
        // Wait for solve to complete
        for line in &response {
            if line.contains("SOLVER: stopped") || line.contains("END") {
                break;
            }
        }
        Ok(())
    }

    /// Query the strategy at a specific node path.
    /// Returns a vector of (action, probability) pairs.
    pub fn get_strategy(&mut self, node_path: &str) -> Result<Vec<(String, f64)>> {
        let response = self.send_command(&format!("show_strategy {}", node_path))?;
        let mut strategies = Vec::new();
        for line in &response {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(prob) = parts[1].parse::<f64>() {
                    strategies.push((parts[0].to_string(), prob));
                }
            }
        }
        Ok(strategies)
    }

    /// Query the expected value at a node.
    pub fn get_ev(&mut self, node_path: &str) -> Result<f64> {
        let response = self.send_command(&format!("show_ev {}", node_path))?;
        for line in &response {
            if let Ok(ev) = line.trim().parse::<f64>() {
                return Ok(ev);
            }
        }
        bail!("Could not parse EV from response: {:?}", response)
    }

    /// Get the exploitability of the current solution.
    pub fn get_exploitability(&mut self) -> Result<f64> {
        let response = self.send_command("calc_exploitability")?;
        for line in &response {
            if let Ok(exp) = line.trim().parse::<f64>() {
                return Ok(exp);
            }
        }
        bail!("Could not parse exploitability from response: {:?}", response)
    }

    /// Shut down the PIOSolver subprocess.
    pub fn quit(mut self) -> Result<()> {
        let _ = self.send_command("exit");
        self.child.wait()?;
        Ok(())
    }

    fn wait_for_ready(&mut self) -> Result<()> {
        let mut buf = String::new();
        loop {
            buf.clear();
            self.stdout.read_line(&mut buf)?;
            if buf.contains("END") || buf.contains("ready") || buf.trim().is_empty() {
                break;
            }
        }
        Ok(())
    }

    fn read_response(&mut self) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = self.stdout.read_line(&mut buf)?;
            if n == 0 {
                break; // EOF
            }
            let line = buf.trim().to_string();
            if line == "END" || line.is_empty() {
                break;
            }
            lines.push(line);
        }
        Ok(lines)
    }
}

impl Drop for UpiClient {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "exit");
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

/// Compare our solver's strategy against PIOSolver's at matching nodes.
#[derive(Debug)]
pub struct ComparisonResult {
    /// Number of nodes compared.
    pub nodes_compared: usize,
    /// Average L1 distance between strategy vectors across all compared nodes.
    pub avg_l1_distance: f64,
    /// Maximum L1 distance seen.
    pub max_l1_distance: f64,
    /// Our solver's exploitability (mbb/hand).
    pub our_exploitability: f64,
    /// PIOSolver's exploitability (mbb/hand), if available.
    pub pio_exploitability: Option<f64>,
}

/// Compute L1 distance between two strategy vectors.
pub fn strategy_l1_distance(a: &[f32], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(&ai, &bi)| (ai as f64 - bi).abs())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l1_distance_identical() {
        let a = vec![0.5f32, 0.3, 0.2];
        let b = vec![0.5f64, 0.3, 0.2];
        let dist = strategy_l1_distance(&a, &b);
        assert!(dist < 1e-6, "Identical strategies should have L1 distance ~0, got {dist}");
    }

    #[test]
    fn l1_distance_different() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f64, 1.0, 0.0];
        let dist = strategy_l1_distance(&a, &b);
        assert!((dist - 2.0).abs() < 1e-6, "Maximally different should be 2.0, got {dist}");
    }
}
