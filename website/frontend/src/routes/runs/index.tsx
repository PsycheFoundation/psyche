import { createFileRoute, Navigate } from '@tanstack/react-router'
import { Runs } from '../../components/Runs.js'
import { fetchSummariesStreaming } from '../../fetchRuns.js'
import { useStreamingLoaderData } from '../../useStreamingData.js'
import { ApiGetRuns } from 'shared'

export const Route = createFileRoute('/runs/')({
	loader: fetchSummariesStreaming,
	component: RouteComponent,
})

function RouteComponent() {
	const runs = useStreamingLoaderData<ApiGetRuns>({
		from: '/runs/',
	})

	if (!runs) {
		return <div>Loading...</div>
	}

	if (runs.runs.length === 1) {
		return (
			<Navigate
				to={'/runs/$run/$index'}
				params={{
					run: runs.runs[0].id,
					index: `${runs.runs[0].index}`,
				}}
			/>
		)
	}

	return <Runs key={window.location.pathname} {...runs} />
}
