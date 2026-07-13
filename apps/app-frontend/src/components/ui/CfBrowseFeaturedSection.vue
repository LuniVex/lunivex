<script setup lang="ts">
import { PlusIcon, SpinnerIcon } from '@modrinth/assets'
import { injectNotificationManager } from '@modrinth/ui'
import { ref } from 'vue'
import { useRouter } from 'vue-router'

import { curseforge_install_modpack } from '@/helpers/curseforge.js'

export type CfFeaturedMod = {
	id: number
	name: string
	summary: string
	logoUrl: string | null
	bannerUrl: string | null
}

defineProps<{
	mods: CfFeaturedMod[]
	label: string
	subtitle?: string
	comments?: Record<number, string>
}>()

const { handleError } = injectNotificationManager()
const router = useRouter()

const installingIds = ref<number[]>([])
const installedIds = ref<number[]>([])

async function handleInstall(mod: CfFeaturedMod) {
	if (installingIds.value.includes(mod.id)) return
	installingIds.value = [...installingIds.value, mod.id]
	try {
		await curseforge_install_modpack(mod.id)
		installedIds.value = [...installedIds.value, mod.id]
	} catch (err) {
		handleError(err as Error)
	} finally {
		installingIds.value = installingIds.value.filter((id) => id !== mod.id)
	}
}

function navigate(id: number) {
	router.push(`/cf-project/${id}`)
}
</script>

<template>
	<div v-if="mods.length > 0" class="featured-section">
		<div class="featured-heading-row">
			<h2 class="featured-heading">{{ label }}</h2>
			<span v-if="subtitle" class="featured-heading-sub">{{ subtitle }}</span>
		</div>
		<div class="featured-cards">
			<div
				v-for="mod in mods"
				:key="mod.id"
				class="featured-card"
				role="link"
				tabindex="0"
				@click="navigate(mod.id)"
				@keydown.enter="navigate(mod.id)"
			>
				<div class="featured-media">
					<img
						v-if="mod.bannerUrl"
						:src="mod.bannerUrl"
						:alt="mod.name"
						class="featured-banner"
					/>
					<img
						v-else-if="mod.logoUrl"
						:src="mod.logoUrl"
						:alt="mod.name"
						class="featured-thumb"
					/>
					<div v-else class="featured-thumb featured-thumb-fallback" />
				</div>
				<div class="featured-info">
					<p class="featured-name">{{ mod.name }}</p>
					<p v-if="comments?.[mod.id]" class="featured-syl-comment">{{ comments[mod.id] }}</p>
					<p class="featured-summary">{{ mod.summary }}</p>
				</div>
				<button
					class="featured-install"
					:disabled="installingIds.includes(mod.id) || installedIds.includes(mod.id)"
					@click.stop="handleInstall(mod)"
				>
					<SpinnerIcon
						v-if="installingIds.includes(mod.id)"
						class="featured-install-icon animate-spin"
					/>
					<PlusIcon v-else class="featured-install-icon" />
					{{
						installedIds.includes(mod.id)
							? 'Installed'
							: installingIds.includes(mod.id)
								? 'Installing...'
								: 'Install'
					}}
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
	border-left: 3px solid var(--source-accent);
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
		border-color: var(--source-accent);
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
	color: #f09070;
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
	background: var(--source-accent);
	color: rgba(0, 0, 0, 0.85);
	border: none;
	border-radius: 0.75rem;
	font-size: 1rem;
	font-weight: 600;
	cursor: pointer;
	white-space: nowrap;
	flex-shrink: 0;
	transition:
		filter 0.15s,
		opacity 0.15s;

	&:hover:not(:disabled) {
		filter: brightness(1.1);
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
