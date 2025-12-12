import { createFileRoute, redirect } from '@tanstack/react-router'
import { Runs } from '../../components/Runs.js'
import { fetchRuns, fetchSummariesStreaming } from '../../fetchRuns.js'
import { useStreamingLoaderData } from '../../useStreamingData.js'
import { ApiGetRuns } from 'shared'

export const Route = createFileRoute('/runs/')({
	loader: async () => {
		const initialRuns = await fetchRuns()
		if (initialRuns.runs.length === 1) {
			throw redirect({
				to: '/runs/$run/$programId/$index',
				params: {
					run: initialRuns.runs[0].id,
					programId: initialRuns.runs[0].programId,
					index: `${initialRuns.runs[0].index}`,
				},
			})
		}
		return fetchSummariesStreaming()
	},
	component: RouteComponent,
})

function RouteComponent() {
	const runs = useStreamingLoaderData<ApiGetRuns>({
		from: '/runs/',
	})

	if (!runs) {
		return <div>Loading...</div>
	}

	return <Runs key={window.location.pathname} {...runs} />
}
