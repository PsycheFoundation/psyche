import { createFileRoute, redirect } from '@tanstack/react-router'
import { Runs } from '../../components/Runs.js'
import { fetchRuns } from '../../fetchRuns.js'

export const Route = createFileRoute('/runs/')({
	loader: () =>
		fetchRuns().then((runs) => {
			if (runs.runs.length === 1) {
				throw redirect({
					to: '/runs/$run/$programId/$index',
					params: {
						run: runs.runs[0].id,
						programId: runs.runs[0].programId,
						index: `${runs.runs[0].index}`,
					},
				})
			}
			return runs
		}),
	component: RouteComponent,
})

function RouteComponent() {
	const runs = Route.useLoaderData()
	return <Runs {...runs} />
}
