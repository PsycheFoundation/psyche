// CheckpointButton.tsx
import { useState, useEffect } from 'react'
import { Button } from './Button.js'
import HuggingfaceIcon from '../assets/icons/huggingface.svg?react'
import { fetchCheckpointStatus } from '../fetchRuns.js'

export const CheckpointButton = ({
	checkpoint,
}: {
	checkpoint: { repo_id: string; revision?: string | null }
}) => {
	const [isValid, setIsValid] = useState<boolean | undefined>(undefined)

	useEffect(() => {
		const [owner, repo] = checkpoint.repo_id.split('/')

		console.log('FETCHING CHECKPOINT STATUS')
		fetchCheckpointStatus(owner, repo, checkpoint.revision || undefined)
			.then((data) => {
				setIsValid(data.isValid)
			})
			.catch(() => {
				setIsValid(false)
			})
	}, [checkpoint.repo_id, checkpoint.revision])

	// Show nothing while loading
	if (isValid === undefined) {
		return null
	}

	// Don't render if invalid
	if (!isValid) {
		return null
	}

	return (
		<Button
			style="secondary"
			center
			icon={{
				side: 'left',
				svg: HuggingfaceIcon,
				autoColor: false,
			}}
			href={`https://huggingface.co/${checkpoint.repo_id}${checkpoint.revision ? `/tree/${checkpoint.revision}` : ''}`}
			target="_blank"
		>
			latest checkpoint: {checkpoint.repo_id.split('/')[1]}
		</Button>
	)
}
