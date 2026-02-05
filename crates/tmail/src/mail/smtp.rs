//! SMTP client wrapper using lettre with rustls

use crate::config::TmailConfig;
use anyhow::{Context, Result};
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

/// SMTP client for sending emails
pub struct SmtpClient {
    transport: Option<AsyncSmtpTransport<Tokio1Executor>>,
    config: TmailConfig,
}

impl SmtpClient {
    /// Create a new SMTP client (not connected)
    pub fn new(config: TmailConfig) -> Self {
        Self {
            transport: None,
            config,
        }
    }

    /// Connect to the SMTP server
    pub async fn connect(&mut self) -> Result<()> {
        let password = self
            .config
            .smtp
            .password
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No SMTP password configured"))?;

        let creds = Credentials::new(self.config.smtp.username.clone(), password);

        // Build transport based on TLS setting and port
        let transport = if self.config.smtp.tls && self.config.smtp.port == 465 {
            // Implicit TLS (SMTPS)
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.smtp.host)
                .context("Failed to create SMTP transport")?
                .credentials(creds)
                .port(self.config.smtp.port)
                .build()
        } else if self.config.smtp.tls {
            // STARTTLS (typically port 587)
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.smtp.host)
                .context("Failed to create SMTP transport")?
                .credentials(creds)
                .port(self.config.smtp.port)
                .build()
        } else {
            // Plain (not recommended)
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.smtp.host)
                .credentials(creds)
                .port(self.config.smtp.port)
                .build()
        };

        self.transport = Some(transport);
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.transport.is_some()
    }

    /// Send an email
    pub async fn send(
        &self,
        to: &[String],
        cc: &[String],
        subject: &str,
        body: &str,
    ) -> Result<()> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SMTP not connected"))?;

        // Build from address with display name
        let from = format!(
            "{} <{}>",
            self.config.display_name, self.config.smtp.username
        );

        let mut builder = Message::builder()
            .from(from.parse().context("Invalid from address")?)
            .subject(subject);

        // Add recipients
        for recipient in to {
            builder = builder.to(recipient.parse().context("Invalid to address")?);
        }
        for recipient in cc {
            builder = builder.cc(recipient.parse().context("Invalid cc address")?);
        }

        // Build body with optional signature
        let full_body = if let Some(ref sig) = self.config.signature {
            format!("{}\n\n--\n{}", body, sig)
        } else {
            body.to_string()
        };

        let email = builder
            .header(ContentType::TEXT_PLAIN)
            .body(full_body)
            .context("Failed to build email")?;

        transport
            .send(email)
            .await
            .context("Failed to send email")?;

        Ok(())
    }
}
