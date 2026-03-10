// CheckpointButton.tsx
import { useState, useEffect } from 'react'
import { Button } from './Button.js'
import HuggingfaceIcon from '../assets/icons/huggingface.svg?react'
import LinkIcon from '../assets/icons/link.svg?react'
import {
	fetchCheckpointStatus,
	fetchGcsCheckpointStatus,
} from '../fetchRuns.js'
import type { GcsRepo, HubRepo } from 'shared'

type CheckpointProps = {
	checkpoint: { Hub: HubRepo } | { Gcs: GcsRepo }
}

export const CheckpointButton = ({ checkpoint }: CheckpointProps) => {
	const [isValid, setIsValid] = useState<boolean | undefined>(undefined)

	const isHub = 'Hub' in checkpoint
	const isGcs = 'Gcs' in checkpoint

	useEffect(() => {
		if (isHub) {
			const repoId = checkpoint.Hub.repo_id
			const parsedRepo = repoId.split('/')

			if (parsedRepo.length !== 2) {
				setIsValid(false)
				return
			}
			const [owner, repo] = parsedRepo

			fetchCheckpointStatus(owner, repo, checkpoint.Hub.revision || undefined)
				.then((data) => {
					setIsValid(data.isValid)
				})
				.catch(() => {
					setIsValid(false)
				})
		} else if (isGcs) {
			const bucket = checkpoint.Gcs.bucket
			const prefix = checkpoint.Gcs.prefix || undefined

			fetchGcsCheckpointStatus(bucket, prefix)
				.then((data) => {
					setIsValid(data.isValid)
				})
				.catch(() => {
					setIsValid(false)
				})
		}
	}, [checkpoint, isHub, isGcs])

	if (isValid === undefined) {
		return null
	}

	// Don't render if invalid
	if (!isValid) {
		return null
	}

	if (isHub) {
		const repoId = checkpoint.Hub.repo_id
		const revision = checkpoint.Hub.revision

		return (
			<Button
				style="secondary"
				center
				icon={{
					side: 'left',
					svg: HuggingfaceIcon,
					autoColor: false,
				}}
				href={`https://huggingface.co/${repoId}${revision ? `/tree/${revision}` : ''}`}
				target="_blank"
			>
				latest checkpoint: {repoId.split('/')[1]}
			</Button>
		)
	}

	if (isGcs) {
		const bucket = checkpoint.Gcs.bucket
		const prefix = checkpoint.Gcs.prefix

		return (
			<Button
				style="secondary"
				center
				icon={{
					side: 'left',
					svg: LinkIcon,
				}}
				href={`https://console.cloud.google.com/storage/browser/${bucket}${prefix ? `/${prefix}` : ''}`}
				target="_blank"
			>
				latest checkpoint: {bucket}
			</Button>
		)
	}

	return null
}
