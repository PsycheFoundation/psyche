# Joining a Run

This section covers everything you need to know to join an existing Psyche training run and start contributing compute.

## What You'll Learn

- **[Requirements](./requirements.md)** - Hardware, software, and service prerequisites for running a Psyche client
- **[Quickstart](./quickstart.md)** - Step-by-step guide to get your client up and running quickly
- **[Troubleshooting](./troubleshooting.md)** - Common issues and how to resolve them
- **[FAQ](./faq.md)** - Frequently asked questions about participating in training runs

## Quick Overview

Joining a Psyche training run involves:

1. Setting up the required software (NVIDIA drivers, Docker, Container Toolkit)
2. Configuring your Solana RPC providers
3. Creating a `.env` file with your settings
4. Running the Psyche Docker container with GPU access

Once your client is running, it will automatically synchronize with the coordinator and participate in training rounds alongside other clients.

You can either specify a specific `RUN_ID` to join, or omit it to let the client automatically discover and join an available run from the coordinator.

## Getting Help

If you encounter issues not covered in the [Troubleshooting](./troubleshooting.md) or [FAQ](./faq.md) sections, you can:

- Check the [Psyche GitHub repository](https://github.com/PsycheFoundation/psyche) for known issues
- Review the [Glossary](../explain/glossary.md) for terminology clarification
- Understand the [Workflow Overview](../explain/workflow-overview.md) to learn how clients interact with the coordinator
