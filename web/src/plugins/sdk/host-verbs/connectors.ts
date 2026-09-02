import { write } from './request'

export interface HostConnectorMessage {
  /** The channel as that connector spells it — a Slack channel id, a chat id. */
  channel: string
  text: string
  /** Reply into a thread rather than to the channel, where the connector has
   *  threads at all. */
  thread?: string
}

export interface PluginHostConnectors {
  /** Send through a configured connector, named as `config.yaml` names it. */
  send(connector: string, message: HostConnectorMessage): Promise<void>
}

export const connectors: PluginHostConnectors = {
  async send(connector, message) {
    // On success the route answers `{ status: "sent" }` and drops the provider's
    // message id, so there is nothing truthful to resolve with; a send that did
    // not land comes back as an error status, and `write` rejects.
    await write<{ status: string }>(
      'connectors.send',
      `/api/connectors/${encodeURIComponent(connector)}/send`,
      message,
    )
  },
}
