<script setup lang="ts">
import { XIcon } from '@modrinth/assets'
import { ref, watch } from 'vue'

import { list as listProfiles } from '@/helpers/profile'
import { resolveIconUrl } from '@/helpers/utils.js'

export type PickerInstance = {
	path: string
	name: string
	loader: string
	gameVersion: string
	iconPath?: string
}

const props = defineProps<{
	open: boolean
	modName: string
	modIconUrl?: string
}>()

const emit = defineEmits<{
	close: []
	select: [instance: PickerInstance]
}>()

const instances = ref<PickerInstance[]>([])

watch(
	() => props.open,
	async (open) => {
		if (!open) return
		try {
			const all = await listProfiles()
			instances.value = all
				.filter((p: { install_stage: string }) => p.install_stage === 'installed')
				.map(
					(p: {
						path: string
						name: string
						loader: string
						game_version: string
						icon_path?: string
					}) => ({
						path: p.path,
						name: p.name,
						loader: p.loader,
						gameVersion: p.game_version,
						iconPath: p.icon_path,
					}),
				)
		} catch {
			instances.value = []
		}
	},
)
</script>

<template>
	<Teleport to="body">
		<div v-if="open" class="cf-picker-overlay" @click.self="emit('close')">
			<div class="cf-picker">
				<div class="cf-picker__header">
					<img v-if="modIconUrl" :src="modIconUrl" class="cf-picker__mod-icon" alt="" />
					<div class="cf-picker__mod-info">
						<p class="cf-picker__mod-title">{{ modName }}</p>
						<p class="cf-picker__mod-subtitle">Select an instance to install to</p>
					</div>
					<button class="cf-picker__close" @click="emit('close')">
						<XIcon class="h-4 w-4" />
					</button>
				</div>
				<div v-if="instances.length === 0" class="cf-picker__empty">
					No installed instances found. Create one first.
				</div>
				<div v-else class="cf-picker__list">
					<button
						v-for="inst in instances"
						:key="inst.path"
						class="cf-picker__instance"
						@click="emit('select', inst)"
					>
						<img
							v-if="inst.iconPath"
							:src="resolveIconUrl(inst.iconPath)"
							class="cf-picker__instance-icon"
							alt=""
						/>
						<div v-else class="cf-picker__instance-icon cf-picker__instance-icon--placeholder" />
						<div class="cf-picker__instance-info">
							<span class="cf-picker__instance-name">{{ inst.name }}</span>
							<span class="cf-picker__instance-meta">
								{{ inst.loader }} {{ inst.gameVersion }}
							</span>
						</div>
					</button>
				</div>
			</div>
		</div>
	</Teleport>
</template>

<style scoped lang="scss">
.cf-picker-overlay {
	position: fixed;
	inset: 0;
	z-index: 1000;
	display: flex;
	align-items: center;
	justify-content: center;
	background: rgba(0, 0, 0, 0.5);
	backdrop-filter: blur(2px);
}

.cf-picker {
	background: var(--color-raised-bg, #1a1626);
	border: 1px solid rgba(142, 132, 207, 0.15);
	border-radius: 1rem;
	width: min(420px, calc(100vw - 2rem));
	max-height: min(480px, calc(100vh - 4rem));
	display: flex;
	flex-direction: column;
	overflow: hidden;

	&__header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem 1rem 0.875rem;
		border-bottom: 1px solid rgba(142, 132, 207, 0.1);
		flex-shrink: 0;
	}

	&__mod-icon {
		width: 2.5rem;
		height: 2.5rem;
		border-radius: var(--radius-sm, 0.4rem);
		object-fit: cover;
		flex-shrink: 0;
	}

	&__mod-info {
		flex: 1;
		min-width: 0;
	}

	&__mod-title {
		margin: 0;
		font-size: 0.9375rem;
		font-weight: 600;
		color: var(--color-base);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	&__mod-subtitle {
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-secondary);
	}

	&__close {
		background: none;
		border: none;
		color: var(--color-secondary);
		cursor: pointer;
		padding: 0.25rem;
		flex-shrink: 0;
		display: flex;
		align-items: center;

		&:hover {
			color: var(--color-base);
		}
	}

	&__empty {
		padding: 2rem 1rem;
		text-align: center;
		color: var(--color-secondary);
		font-size: 0.875rem;
	}

	&__list {
		overflow-y: auto;
		padding: 0.5rem;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	&__instance {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.5rem 0.625rem;
		border-radius: 0.5rem;
		border: none;
		background: transparent;
		cursor: pointer;
		text-align: left;
		width: 100%;
		transition: background 0.1s;

		&:hover {
			background: rgba(142, 132, 207, 0.1);
		}
	}

	&__instance-icon {
		width: 2rem;
		height: 2rem;
		border-radius: 0.3rem;
		object-fit: cover;
		flex-shrink: 0;

		&--placeholder {
			background: rgba(142, 132, 207, 0.15);
		}
	}

	&__instance-info {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	&__instance-name {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-base);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	&__instance-meta {
		font-size: 0.75rem;
		color: var(--color-secondary);
		text-transform: capitalize;
	}
}
</style>
