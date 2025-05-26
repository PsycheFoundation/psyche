import { PublicKey } from '@solana/web3.js'

const reconnectDelayMs = 5000
const heartbeatIntervalMs = 30000

export class ProgramEventListener {
	private rpcWebsocketEndpoint: string
	private programId: PublicKey
	private name: string

	private websocket: WebSocket | null = null
	private reconnectTimeoutId: NodeJS.Timeout | null = null
	private heartbeatTimeoutId: NodeJS.Timeout | null = null
	private nextTick: {
		resolve: () => void
		resolved: Promise<void>
	}
	private lastReceivedTime = 0

	constructor(
		rpcWebsocketEndpoint: string,
		programId: PublicKey,
		name: string
	) {
		this.name = name
		this.rpcWebsocketEndpoint = rpcWebsocketEndpoint
		this.programId = programId
		let resolve!: () => void
		const resolved = new Promise<void>((r) => {
			resolve = r
		})
		this.nextTick = {
			resolve,
			resolved,
		}
		this.connect()
	}

	private connect() {
		if (this.websocket) {
			this.cleanup()
		}

		const ws = new WebSocket(this.rpcWebsocketEndpoint)
		this.websocket = ws

		console.log(
			`[${this.name}] [ws] Attempting to connect to RPC websocket at url ${this.rpcWebsocketEndpoint}`
		)
		ws.onopen = () => {
			console.log(
				`[${this.name}] [ws] Connection established for RPC ${this.rpcWebsocketEndpoint}`
			)

			ws.send(
				JSON.stringify({
					jsonrpc: '2.0',
					id: 1,
					method: 'programSubscribe',
					params: [
						this.programId.toBase58(),
						{ commitment: 'confirmed', encoding: 'base64' },
					],
				})
			)

			this.startHeartbeat()
		}

		this.websocket.onmessage = (event) => {
			try {
				const data = JSON.parse(event.data)

				if (data.id === 1 && data.result) {
					console.log(
						`[${this.name}] [ws] Program subscription confirmed:`,
						data.result
					)
				}

				this.lastReceivedTime = Date.now()

				if (data.method === 'programNotification') {
					this.nextTick.resolve()
					let resolve!: () => void
					const resolved = new Promise<void>((r) => {
						resolve = r
					})
					this.nextTick = {
						resolve,
						resolved,
					}
				}
			} catch (error) {
				console.error(`[${this.name}] [ws] Error processing message:`, error)
			}
		}

		this.websocket.onerror = (error) => {
			console.error(
				`[${this.name}] [ws] Error when connecting to ${this.rpcWebsocketEndpoint} :`,
				error
			)
			this.reconnect()
		}

		this.websocket.onclose = () => {
			console.log(
				`[${this.name}] [ws] Connection closed from endpoint ${this.rpcWebsocketEndpoint}`
			)
			this.reconnect()
		}
	}

	private startHeartbeat() {
		if (this.heartbeatTimeoutId) {
			clearTimeout(this.heartbeatTimeoutId)
		}

		this.heartbeatTimeoutId = setTimeout(() => {
			if (this.websocket && this.websocket.readyState === WebSocket.OPEN) {
				this.websocket.send(
					JSON.stringify({
						jsonrpc: '2.0',
						id: 'heartbeat',
						method: 'ping',
					})
				)

				// if we haven't received anything in a while
				const timeSinceLastReceivedMs = Date.now() - this.lastReceivedTime
				if (timeSinceLastReceivedMs > heartbeatIntervalMs * 2) {
					console.warn(
						`[${this.name}] [ws] No events received for too long, reconnecting...`
					)
					this.reconnect()
					return
				}
			} else {
				this.reconnect()
				return
			}

			this.startHeartbeat()
		}, heartbeatIntervalMs)
	}

	private reconnect() {
		if (this.reconnectTimeoutId) {
			return // already planning to reconnect
		}

		this.cleanup()

		this.reconnectTimeoutId = setTimeout(() => {
			this.reconnectTimeoutId = null
			console.log(`[${this.name}] [ws] Attempting to reconnect WebSocket...`)
			this.connect()
		}, reconnectDelayMs)
	}

	private cleanup() {
		if (this.websocket) {
			this.websocket.onclose = null
			this.websocket.onerror = null
			this.websocket.onmessage = null
			if (
				this.websocket.readyState === WebSocket.OPEN ||
				this.websocket.readyState === WebSocket.CONNECTING
			) {
				this.websocket.close()
			}
			this.websocket = null
		}

		if (this.heartbeatTimeoutId) {
			clearTimeout(this.heartbeatTimeoutId)
			this.heartbeatTimeoutId = null
		}
	}

	public nextUpdate(): Promise<void> {
		return this.nextTick.resolved
	}

	public disconnect() {
		this.cleanup()

		if (this.reconnectTimeoutId) {
			clearTimeout(this.reconnectTimeoutId)
			this.reconnectTimeoutId = null
		}
	}
}

export default ProgramEventListener
