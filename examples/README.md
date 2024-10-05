# examples
This example substream has several modules that showcases `substream-solana-utils` usage.

List of the available modules:
- `instruction_logs`: Read the logs of transactions in a per instruction basis.

## Usage
1. Setup the environment variable `STREAMINGFAST_KEY` with an [API key](https://app.streamingfast.io/keys).
2. Run `. ./token.sh`
3. Start streaming with `make stream <module> START=<slot>`. You can verify the most recent slot on the [Solana Explorer](https://explorer.solana.com).

For instance, run `instruction_logs` module with `make stream instruction_logs START=293932407`.
