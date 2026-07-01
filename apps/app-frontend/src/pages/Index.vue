<script setup lang="ts">
import { PlusIcon, SparklesIcon, SpinnerIcon } from '@modrinth/assets'
import { injectNotificationManager } from '@modrinth/ui'
import { invoke } from '@tauri-apps/api/core'
import { readDir } from '@tauri-apps/plugin-fs'
import dayjs from 'dayjs'
import { computed, inject, onUnmounted, ref } from 'vue'
import { useRoute } from 'vue-router'

import Instance from '@/components/ui/Instance.vue'
import RecentWorldsList from '@/components/ui/world/RecentWorldsList.vue'
import { profile_listener } from '@/helpers/events'
import { get_full_path, list } from '@/helpers/profile.js'
import type { GameInstance } from '@/helpers/types'
import {
	appUpdateState,
	downloadAvailableAppUpdate,
	installAvailableAppUpdate,
} from '@/providers/app-update'
import { useBreadcrumbs } from '@/store/breadcrumbs'

const { handleError } = injectNotificationManager()
const showCreationModal = inject<(() => void) | undefined>('showCreationModal')
const route = useRoute()
const breadcrumbs = useBreadcrumbs()

breadcrumbs.setRootContext({ name: 'Home', link: route.path })

// ── App updater banner ───────────────────────────────────────────────────────

const {
	isVisible: isUpdateVisible,
	availableUpdate,
	finishedDownloading,
	downloading,
} = appUpdateState

const updateBannerDismissed = ref(false)
const isUpdateBannerVisible = computed(() => isUpdateVisible.value && !updateBannerDismissed.value)
const isUpdateDownloading = computed(() => downloading.value && !finishedDownloading.value)

async function handleUpdateAction() {
	if (finishedDownloading.value) {
		await installAvailableAppUpdate()
	} else {
		await downloadAvailableAppUpdate()
	}
}

// ── Instances ────────────────────────────────────────────────────────────────

const instances = ref<GameInstance[]>([])

const recentInstances = computed(() =>
	instances.value
		.filter((x) => x.last_played)
		.slice()
		.sort((a, b) => dayjs(b.last_played).diff(dayjs(a.last_played))),
)

const offline = ref<boolean>(!navigator.onLine)
window.addEventListener('offline', () => {
	offline.value = true
})
window.addEventListener('online', () => {
	offline.value = false
})

// ── Essential mod installer ──────────────────────────────────────────────────

const ESSENTIAL_SUPPORTED_LOADERS = new Set(['fabric', 'forge', 'neoforge'])
const ESSENTIAL_VERSION_FALLBACK = '1.4.0.3'

async function fetchEssentialVersion(): Promise<string> {
	const SESSION_KEY = 'lv-essential-version'
	const cached = sessionStorage.getItem(SESSION_KEY)
	if (cached) return cached

	try {
		const resp = await fetch(
			'https://downloads.essential.gg/v1/mods/essential/essential-pinned/updates/latest',
		)
		if (resp.ok) {
			const text = await resp.text()
			// Try JSON response with common version field names
			try {
				const data = JSON.parse(text) as Record<string, unknown>
				const version = (data.version ?? data.tag ?? data.name ?? data.id) as string | undefined
				if (typeof version === 'string' && /^\d+\.\d+/.test(version)) {
					sessionStorage.setItem(SESSION_KEY, version)
					return version
				}
			} catch {
				// Not JSON - check if the body itself is a bare version string
				const trimmed = text.trim()
				if (/^\d+\.\d+/.test(trimmed)) {
					sessionStorage.setItem(SESSION_KEY, trimmed)
					return trimmed
				}
			}
			// If fetch followed a redirect, extract version from the final URL
			const match = resp.url.match(/\/updates\/(\d[^/]+)/)
			if (match?.[1]) {
				sessionStorage.setItem(SESSION_KEY, match[1])
				return match[1]
			}
		}
	} catch {
		// Network error or CORS - fall back silently
	}

	return ESSENTIAL_VERSION_FALLBACK
}

const essentialPrompts = ref<GameInstance[]>([])
const essentialInstalling = ref(new Set<string>())
const essentialSucceeded = ref(new Set<string>())

function isEssentialAsked(path: string): boolean {
	return !!localStorage.getItem(`lv-essential-asked:${path}`)
}

function markEssentialAsked(path: string): void {
	localStorage.setItem(`lv-essential-asked:${path}`, '1')
}

function essentialSlug(loader: string, mcVersion: string): string {
	// Slug format: fabric_1-20-1, forge_1-20-1, neoforge_1-20-4, fabric_26-2
	return `${loader}_${mcVersion.replace(/\./g, '-')}`
}

async function hasEssentialInstalled(instance: GameInstance): Promise<boolean> {
	try {
		const profileDir = await get_full_path(instance.path)
		const sep = profileDir.includes('\\') ? '\\' : '/'
		const entries = await readDir(`${profileDir}${sep}mods`)
		return entries.some((e) => {
			const name = e.name?.toLowerCase() ?? ''
			return name.includes('essential') && name.endsWith('.jar')
		})
	} catch {
		return false
	}
}

async function checkEssentialForInstance(instance: GameInstance): Promise<void> {
	if (instance.install_stage !== 'installed') return
	if (!ESSENTIAL_SUPPORTED_LOADERS.has(instance.loader)) return
	if (isEssentialAsked(instance.path)) return
	if (await hasEssentialInstalled(instance)) {
		markEssentialAsked(instance.path)
		return
	}
	if (!essentialPrompts.value.some((p) => p.path === instance.path)) {
		essentialPrompts.value.push(instance)
	}
}

async function checkAllEssential(): Promise<void> {
	essentialPrompts.value = essentialPrompts.value.filter((p) =>
		instances.value.some((i) => i.path === p.path),
	)
	for (const instance of instances.value) {
		await checkEssentialForInstance(instance)
	}
}

async function installEssential(instance: GameInstance): Promise<void> {
	essentialInstalling.value = new Set([...essentialInstalling.value, instance.path])

	try {
		const mcVersion = instance.game_version
		if (!mcVersion) throw new Error(`Instance "${instance.name}" has no game_version`)

		const essentialVersion = await fetchEssentialVersion()
		const slug = essentialSlug(instance.loader, mcVersion)
		const downloadUrl = `https://downloads.essential.gg/v1/mods/essential/essential-pinned/updates/${essentialVersion}/${slug}/download`

		const profileDir = await get_full_path(instance.path)
		const sep = profileDir.includes('\\') ? '\\' : '/'
		const jarName = `essential-${essentialVersion}-${slug}.jar`
		const jarPath = `${profileDir}${sep}mods${sep}${jarName}`

		console.log('[Essential] Invoking download_file:', downloadUrl, '->', jarPath)
		await invoke('plugin:files|download_file', { url: downloadUrl, destPath: jarPath })
		console.log('[Essential] Installed at:', jarPath)

		markEssentialAsked(instance.path)
		essentialSucceeded.value = new Set([...essentialSucceeded.value, instance.path])
		setTimeout(() => {
			essentialPrompts.value = essentialPrompts.value.filter((p) => p.path !== instance.path)
			const next = new Set(essentialSucceeded.value)
			next.delete(instance.path)
			essentialSucceeded.value = next
		}, 3000)
	} catch (err) {
		console.error('[Essential] Install failed:', err)
		handleError(err as Error)
	} finally {
		const next = new Set(essentialInstalling.value)
		next.delete(instance.path)
		essentialInstalling.value = next
	}
}

function dismissEssential(path: string): void {
	markEssentialAsked(path)
	essentialPrompts.value = essentialPrompts.value.filter((p) => p.path !== path)
}

// ── Instance lifecycle ───────────────────────────────────────────────────────

async function fetchInstances() {
	instances.value = (await list().catch(handleError)) ?? []
	void checkAllEssential()
}

await fetchInstances()

const unlistenProfile = await profile_listener(async () => {
	await fetchInstances()
})

function onInstanceDeleted(path: string) {
	instances.value = instances.value.filter((i) => i.path !== path)
	essentialPrompts.value = essentialPrompts.value.filter((p) => p.path !== path)
}

onUnmounted(() => {
	unlistenProfile()
})
</script>

<template>
	<div class="p-6 flex flex-col gap-5">
		<!-- App update banner -->
		<div v-if="isUpdateBannerVisible" class="app-update-banner">
			<span class="app-update-text">
				LuniVex {{ availableUpdate?.version ? `v${availableUpdate.version}` : 'update' }} is
				available
			</span>
			<div class="app-update-actions">
				<button class="app-update-btn" :disabled="isUpdateDownloading" @click="handleUpdateAction">
					{{
						finishedDownloading
							? 'Restart to update'
							: isUpdateDownloading
								? 'Downloading…'
								: 'Update now'
					}}
				</button>
				<button
					class="app-update-dismiss"
					aria-label="Dismiss"
					@click="updateBannerDismissed = true"
				>
					✕
				</button>
			</div>
		</div>

		<div v-if="recentInstances.length > 0">
			<RecentWorldsList :recent-instances="recentInstances" />
		</div>

		<div>
			<div class="flex items-center justify-between mb-2">
				<h2 class="section-heading m-0 text-xl font-bold">
					{{ instances.length > 0 ? 'Your instances' : 'Welcome to LuniVex!' }}
				</h2>
			</div>

			<template v-if="instances.length > 0">
				<section class="instances-grid">
					<Instance
						v-for="instance in instances"
						:key="instance.path + instance.install_stage"
						:instance="instance"
						@deleted="onInstanceDeleted"
					/>
				</section>
			</template>
			<div v-else class="empty-state">
				<p class="empty-msg">No instances yet - create one to get started.</p>
				<button
					class="create-btn"
					:disabled="offline"
					@click="showCreationModal && showCreationModal()"
				>
					<PlusIcon class="create-btn-icon" />
					Create instance
				</button>
			</div>
		</div>

		<!-- Essential mod prompts -->
		<div v-if="essentialPrompts.length > 0" class="essential-prompts">
			<div
				v-for="prompt in essentialPrompts"
				:key="prompt.path"
				:class="[
					'essential-banner',
					{ 'essential-banner--success': essentialSucceeded.has(prompt.path) },
				]"
			>
				<template v-if="essentialSucceeded.has(prompt.path)">
					<span class="essential-success-check" aria-hidden="true">✓</span>
					<span class="essential-success-msg"
						>Essential added to <strong>{{ prompt.name }}</strong
						>!</span
					>
				</template>
				<template v-else>
					<SparklesIcon class="essential-icon" aria-hidden="true" />
					<div class="essential-text">
						<strong class="essential-name">{{ prompt.name }}</strong>
						<span class="essential-msg"
							>Syl recommends this - makes playing with friends way easier.</span
						>
					</div>
					<div class="essential-actions">
						<button
							class="essential-btn essential-btn--add"
							:disabled="essentialInstalling.has(prompt.path)"
							@click="installEssential(prompt)"
						>
							<SpinnerIcon
								v-if="essentialInstalling.has(prompt.path)"
								class="essential-spinner animate-spin"
							/>
							{{ essentialInstalling.has(prompt.path) ? 'Installing…' : 'Add Essential' }}
						</button>
						<button
							class="essential-btn essential-btn--dismiss"
							:disabled="essentialInstalling.has(prompt.path)"
							@click="dismissEssential(prompt.path)"
						>
							No thanks
						</button>
					</div>
				</template>
			</div>
		</div>
	</div>
</template>

<style lang="scss" scoped>
.instances-grid {
	display: grid;
	grid-template-columns: repeat(auto-fill, minmax(16rem, 18rem));
	gap: 0.75rem;
	width: 100%;
}

.section-heading {
	border-left: 3px solid #8e84cf;
	padding-left: 10px;
	color: #f3f5ff;
}

.empty-state {
	display: flex;
	flex-direction: column;
	align-items: center;
	justify-content: center;
	padding: 3rem 0;
	gap: 1rem;
}

.empty-msg {
	margin: 0;
	color: #9892b0;
	font-size: 14px;
	text-align: center;
}

.create-btn {
	display: flex;
	align-items: center;
	gap: 6px;
	padding: 8px 18px;
	background: #8e84cf;
	color: #0e0d16;
	border: none;
	border-radius: 10px;
	font-size: 14px;
	font-weight: 600;
	cursor: pointer;
	transition: background 0.15s;

	&:hover:not(:disabled) {
		background: #a89fe0;
	}

	&:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
}

.create-btn-icon {
	width: 14px;
	height: 14px;
}

/* App update banner */

.app-update-banner {
	display: flex;
	align-items: center;
	justify-content: space-between;
	gap: 12px;
	padding: 10px 14px;
	background: rgba(142, 132, 207, 0.1);
	border: 1px solid rgba(142, 132, 207, 0.3);
	border-radius: 10px;
}

.app-update-text {
	font-size: 13px;
	color: #c8c2e8;
	flex: 1;
}

.app-update-actions {
	display: flex;
	align-items: center;
	gap: 6px;
	flex-shrink: 0;
}

.app-update-btn {
	padding: 5px 14px;
	background: #8e84cf;
	color: #0e0d16;
	border: none;
	border-radius: 7px;
	font-size: 12px;
	font-weight: 600;
	cursor: pointer;
	transition: background 0.15s;

	&:hover:not(:disabled) {
		background: #a89fe0;
	}

	&:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}
}

.app-update-dismiss {
	padding: 4px 8px;
	background: transparent;
	color: #6b6485;
	border: none;
	border-radius: 6px;
	font-size: 12px;
	cursor: pointer;
	line-height: 1;
	transition: color 0.15s;

	&:hover {
		color: #9892b0;
	}
}

/* Essential prompts */

.essential-prompts {
	display: flex;
	flex-direction: column;
	gap: 8px;
}

.essential-banner {
	display: flex;
	align-items: center;
	gap: 12px;
	padding: 12px 16px;
	background: #221c33;
	border: 1px solid #2a2440;
	border-radius: 16px;
}

.essential-icon {
	width: 18px;
	height: 18px;
	color: #8e84cf;
	flex-shrink: 0;
}

.essential-text {
	display: flex;
	align-items: baseline;
	gap: 6px;
	flex: 1;
	min-width: 0;
	flex-wrap: wrap;
}

.essential-name {
	color: #f3f5ff;
	font-size: 13px;
	white-space: nowrap;
}

.essential-msg {
	color: #9892b0;
	font-size: 13px;
}

.essential-actions {
	display: flex;
	gap: 6px;
	flex-shrink: 0;
}

.essential-banner--success {
	background: rgba(74, 222, 128, 0.06);
	border-color: rgba(74, 222, 128, 0.2);
	animation: essential-fade-out 3s ease-out forwards;
}

.essential-success-check {
	font-size: 15px;
	color: #4ade80;
	flex-shrink: 0;
}

.essential-success-msg {
	font-size: 13px;
	color: #9892b0;

	strong {
		color: #f3f5ff;
	}
}

.essential-spinner {
	width: 11px;
	height: 11px;
	flex-shrink: 0;
}

@keyframes essential-fade-out {
	0%,
	80% {
		opacity: 1;
	}
	100% {
		opacity: 0;
	}
}

.essential-btn {
	display: inline-flex;
	align-items: center;
	gap: 4px;
	padding: 5px 12px;
	border: none;
	border-radius: 7px;
	font-size: 12px;
	font-weight: 500;
	cursor: pointer;
	transition: background 0.15s;

	&--add {
		background: #8e84cf;
		color: #0e0d16;

		&:hover:not(:disabled) {
			background: #a89fe0;
		}

		&:disabled {
			opacity: 0.6;
			cursor: not-allowed;
		}
	}

	&--dismiss {
		background: rgba(255, 255, 255, 0.06);
		color: #9892b0;

		&:hover {
			background: rgba(255, 255, 255, 0.1);
		}
	}
}
</style>
