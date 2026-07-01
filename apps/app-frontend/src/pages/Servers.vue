<template>
	<div class="hosting-placeholder">
		<div class="hosting-placeholder-content">
			<svg width="48" height="48" viewBox="0 0 48 48" aria-hidden="true">
				<defs>
					<mask id="serversCrescentMask">
						<rect width="48" height="48" fill="black" />
						<circle cx="24" cy="24" r="19" fill="white" />
						<circle cx="32" cy="19" r="17" fill="black" />
					</mask>
				</defs>
				<rect
					width="48"
					height="48"
					mask="url(#serversCrescentMask)"
					fill="#B8B0E8"
					opacity="0.4"
				/>
			</svg>
			<h1 class="hosting-title">Server hosting is not available yet.</h1>
			<p class="hosting-sub">Join the Discord for updates.</p>
		</div>
	</div>
</template>

<script setup lang="ts">
import { useRoute } from 'vue-router'

import { useBreadcrumbs } from '@/store/breadcrumbs'

const route = useRoute()
const breadcrumbs = useBreadcrumbs()
breadcrumbs.setRootContext({ name: 'Servers', link: route.path })
</script>

<style scoped lang="scss">
.hosting-placeholder {
	display: flex;
	align-items: center;
	justify-content: center;
	height: 100%;
	width: 100%;
	padding: 2rem;
}

.hosting-placeholder-content {
	display: flex;
	flex-direction: column;
	align-items: center;
	gap: 0.75rem;
	text-align: center;
	max-width: 360px;
}

.hosting-title {
	margin: 0;
	font-size: 1.1rem;
	font-weight: 600;
	color: #f3f5ff;
}

.hosting-sub {
	margin: 0;
	font-size: 0.875rem;
	color: #9892b0;
}
</style>
