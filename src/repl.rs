use anyhow::Result;
use rig::completion::{Chat, Message};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};

pub const AGENT_PREAMBLE: &str = r#"
You are a concise support agent for the RigAgent demo store.
Use retrieved support documents when relevant.
Call lookup_order_status for order status, tracking, carrier, or fulfillment questions.
Call list_orders to see valid order ids for lookup_order_status.
Cite support document title and source when retrieved context informs an answer.
If context or tools do not contain the answer, say what is missing.
"#;

pub async fn run<A>(agent: A) -> Result<()>
where
    A: Chat,
{
    let stdin = io::BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = io::stdout();
    let mut history: Vec<Message> = Vec::new();

    stdout
        .write_all(b"RigAgent support demo. Type /help for commands.\n")
        .await?;

    loop {
        stdout.write_all(b"> ").await?;
        stdout.flush().await?;

        let Some(line) = lines.next_line().await? else {
            break;
        };
        let input = line.trim();

        match input {
            "" => continue,
            "/help" => write_help(&mut stdout).await?,
            "/quit" | "/exit" => break,
            prompt if prompt.starts_with('/') => {
                stdout
                    .write_all(b"Unknown command. Type /help for commands.\n")
                    .await?;
            }
            prompt => match agent.chat(prompt, &mut history).await {
                Ok(response) => {
                    stdout.write_all(response.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                }
                Err(error) => {
                    stdout
                        .write_all(format!("Agent error: {error}\n").as_bytes())
                        .await?;
                }
            },
        }
    }

    stdout.write_all(b"bye\n").await?;
    Ok(())
}

async fn write_help(stdout: &mut io::Stdout) -> Result<()> {
    stdout
        .write_all(
            b"Commands:\n  /help      show commands\n  /quit      exit\n\nTry:\n  What is the return policy for accessories?\n  Where is order RIG-1001?\n",
        )
        .await?;
    Ok(())
}
