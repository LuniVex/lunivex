<script setup lang="ts">
import { PlusIcon, SpinnerIcon } from '@modrinth/assets'
import { injectNotificationManager } from '@modrinth/ui'
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { injectContentInstall } from '@/providers/content-install'

const props = defineProps<{
	slugs: string[]
	label: string
	subtitle?: string
	comments?: Record<string, string>
}>()

const { handleError } = injectNotificationManager()
const { install } = injectContentInstall()
const router = useRouter()

type FeaturedProject = {
	id: string
	slug: string
	title: string
	summary: string
	icon_url: string | null
	featured_gallery: string | null
	gallery: Array<{ url: string; featured: boolean }> | null
}

const projects = ref<FeaturedProject[]>([])
const installingIds = ref<string[]>([])

onMounted(async () => {
	try {
		const results = await Promise.all(
			props.slugs.map((slug) =>
				fetch(`https://api.modrinth.com/v2/project/${slug}`, {
					headers: { 'User-Agent': 'LuniVex/1.0 (https://lunivex.xyz)' },
				})
					.then((r) => (r.ok ? r.json() : null))
					.catch(() => null),
			),
		)
		projects.value = results.filter((r): r is FeaturedProject => !!r && typeof r.title === 'string')
	} catch {
		// best-effort
	}
})

function getBannerUrl(project: FeaturedProject): string | null {
	return (
		project.featured_gallery ??
		project.gallery?.find((g) => g.featured)?.url ??
		project.gallery?.[0]?.url ??
		null
	)
}

async function handleInstall(project: FeaturedProject) {
	if (installingIds.value.includes(project.id)) return
	installingIds.value = [...installingIds.value, project.id]
	try {
		await install(project.id, null, null, 'FeaturedCard')
	} catch (err) {
		handleError(err as Error)
	} finally {
		installingIds.value = installingIds.value.filter((id) => id !== project.id)
	}
}

function navigate(slug: string) {
	router.push(`/project/${slug}`)
}
</script>

<template>
	<div v-if="projects.length > 0" class="featured-section">
		<div class="featured-heading-row">
			<h2 class="featured-heading">{{ label }}</h2>
			<span v-if="subtitle" class="featured-heading-sub">{{ subtitle }}</span>
		</div>
		<div class="featured-cards">
			<div
				v-for="project in projects"
				:key="project.id"
				class="featured-card"
				role="link"
				tabindex="0"
				@click="navigate(project.slug)"
				@keydown.enter="navigate(project.slug)"
			>
				<!-- Gallery banner if available, else icon -->
				<div class="featured-media">
					<img
						v-if="getBannerUrl(project)"
						:src="getBannerUrl(project)!"
						:alt="project.title"
						class="featured-banner"
					/>
					<img
						v-else-if="project.icon_url"
						:src="project.icon_url"
						:alt="project.title"
						class="featured-thumb"
					/>
					<div v-else class="featured-thumb featured-thumb-fallback" />
				</div>
				<div class="featured-info">
					<p class="featured-name">{{ project.title }}</p>
					<p v-if="comments?.[project.slug]" class="featured-syl-comment">{{ comments[project.slug] }}</p>
					<p class="featured-summary">{{ project.summary }}</p>
				</div>
				<button
					class="featured-install"
					:disabled="installingIds.includes(project.id)"
					@click.stop="handleInstall(project)"
				>
					<SpinnerIcon
						v-if="installingIds.includes(project.id)"
						class="featured-install-icon animate-spin"
					/>
					<PlusIcon v-else class="featured-install-icon" />
					{{ installingIds.includes(project.id) ? 'Installing...' : 'Install' }}
				</button>
			</div>
		</div>
	</div>
</template>

<style scoped lang="scss">
.featured-section {
	display: flex;
	flex-direction: column;
	gap: 0.625rem;
}

.featured-heading-row {
	display: flex;
	align-items: baseline;
	gap: 0.625rem;
}

.featured-heading {
	margin: 0;
	font-size: 1rem;
	font-weight: 700;
	color: #f3f5ff;
	border-left: 3px solid #8e84cf;
	padding-left: 10px;
}

.featured-heading-sub {
	font-size: 0.8125rem;
	color: #9892b0;
}

.featured-cards {
	display: flex;
	flex-direction: column;
	gap: 0.75rem;
}

.featured-card {
	display: grid;
	grid-template-columns: 80px 1fr auto;
	align-items: center;
	gap: 0.875rem;
	padding: 1rem;
	background: #221c33;
	border: 1px solid #2a2440;
	border-radius: 16px;
	cursor: pointer;
	transition: border-color 0.125s;

	&:hover {
		border-color: #8e84cf;
	}
}

.featured-media {
	width: 80px;
	height: 80px;
	border-radius: 10px;
	overflow: hidden;
	flex-shrink: 0;
	background: #2a2440;
	display: flex;
	align-items: center;
	justify-content: center;
}

.featured-banner {
	width: 100%;
	height: 100%;
	object-fit: cover;
	object-position: center;
}

.featured-thumb {
	width: 80px;
	height: 80px;
	border-radius: 10px;
	object-fit: cover;
	flex-shrink: 0;
}

.featured-thumb-fallback {
	background: #2a2440;
}

.featured-info {
	min-width: 0;
}

.featured-name {
	margin: 0 0 3px;
	font-size: 0.9375rem;
	font-weight: 600;
	color: #f3f5ff;
	white-space: nowrap;
	overflow: hidden;
	text-overflow: ellipsis;
}

.featured-syl-comment {
	margin: 0 0 3px;
	font-size: 0.75rem;
	color: #a89fe0;
	font-style: italic;
	line-height: 1.3;
	white-space: nowrap;
	overflow: hidden;
	text-overflow: ellipsis;
}

.featured-summary {
	margin: 0;
	font-size: 0.8125rem;
	color: #9892b0;
	display: -webkit-box;
	-webkit-line-clamp: 1;
	-webkit-box-orient: vertical;
	overflow: hidden;
	line-height: 1.45;
}

.featured-install {
	display: inline-flex;
	align-items: center;
	gap: 0.375rem;
	height: 2.25rem;
	padding: 0.5rem 0.625rem;
	background: #8e84cf;
	color: #0e0d16;
	border: none;
	border-radius: 0.75rem;
	font-size: 1rem;
	font-weight: 600;
	cursor: pointer;
	white-space: nowrap;
	flex-shrink: 0;
	transition:
		background 0.15s,
		opacity 0.15s;

	&:hover:not(:disabled) {
		background: #a89fe0;
	}

	&:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}
}

.featured-install-icon {
	width: 1.25rem;
	height: 1.25rem;
	flex-shrink: 0;
}
</style>
