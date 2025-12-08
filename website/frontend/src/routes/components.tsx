import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/components')({
	component: RouteComponent,
})

function RouteComponent() {
	return <div>Hello "/components"!</div>
}
