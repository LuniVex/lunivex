<script>
// Shared across all instances — only one context menu can be open at a time
let activeCloseContextMenu = null
</script>

<script setup>
import {
	DownloadIcon,
	FolderOpenIcon,
	GameIcon,
	PencilIcon,
	PlayIcon,
	SpinnerIcon,
	StopCircleIcon,
	TimerIcon,
	TrashIcon,
} from '@modrinth/assets'
import { Avatar, ButtonStyled, injectNotificationManager, useRelativeTime } from '@modrinth/ui'
import { convertFileSrc } from '@tauri-apps/api/core'
import dayjs from 'dayjs'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { trackEvent } from '@/helpers/analytics'
import { get_project } from '@/helpers/cache'
import { process_listener } from '@/helpers/events'
import { get_by_profile_path } from '@/helpers/process'
import { finish_install, kill, remove, run } from '@/helpers/profile'
import { showProfileInFolder } from '@/helpers/utils.js'
import { handleSevereError } from '@/store/error.js'

const { handleError } = injectNotificationManager()
const formatRelativeTime = useRelativeTime()

const props = defineProps({
	instance: {
		type: Object,
		default() {
			return {}
		},
	},
	compact: {
		type: Boolean,
		default: false,
	},
	first: {
		type: Boolean,
		default: false,
	},
})

const emit = defineEmits(['deleted'])

const playing = ref(false)
const loading = ref(false)
const modLoading = computed(
	() =>
		loading.value ||
		currentEvent.value === 'installing' ||
		(currentEvent.value === 'launched' && !playing.value),
)
const installing = computed(() => props.instance.install_stage.includes('installing'))
const installed = computed(() => props.instance.install_stage === 'installed')

const router = useRouter()

const seeInstance = async () => {
	await router.push(`/instance/${encodeURIComponent(props.instance.path)}`)
}

const checkProcess = async () => {
	const runningProcesses = await get_by_profile_path(props.instance.path).catch(handleError)
	playing.value = runningProcesses.length > 0
}

const play = async (e, context) => {
	e?.stopPropagation()
	loading.value = true
	await run(props.instance.path)
		.catch((err) => handleSevereError(err, { profilePath: props.instance.path }))
		.finally(() => {
			trackEvent('InstanceStart', {
				loader: props.instance.loader,
				game_version: props.instance.game_version,
				source: context,
			})
		})
	loading.value = false
}

const stop = async (e, context) => {
	e?.stopPropagation()
	playing.value = false
	await kill(props.instance.path).catch(handleError)
	trackEvent('InstanceStop', {
		loader: props.instance.loader,
		game_version: props.instance.game_version,
		source: context,
	})
}

const repair = async (e) => {
	e?.stopPropagation()
	await finish_install(props.instance).catch(handleError)
}

const openFolder = async () => {
	await showProfileInFolder(props.instance.path)
}

const addContent = async () => {
	await router.push({
		path: `/browse/${props.instance.loader === 'vanilla' ? 'datapack' : 'mod'}`,
		query: { i: props.instance.path },
	})
}

defineExpose({
	play,
	stop,
	seeInstance,
	openFolder,
	addContent,
	instance: props.instance,
})

// ── Banner image ─────────────────────────────────────────────────────────────

const bannerUrl = ref(null)

const currentEvent = ref(null)

const unlisten = await process_listener((e) => {
	if (e.profile_path_id === props.instance.path) {
		currentEvent.value = e.event
		if (e.event === 'finished') {
			playing.value = false
		}
	}
})

onMounted(async () => {
	checkProcess()
	if (props.instance.linked_data?.project_id) {
		try {
			const project = await get_project(props.instance.linked_data.project_id, 'must_revalidate')
			bannerUrl.value = project?.featured_gallery ?? project?.gallery?.[0]?.url ?? null
		} catch {
			// banner is best-effort
		}
	}
})

// ── Context menu ─────────────────────────────────────────────────────────────

const contextMenuVisible = ref(false)
const contextMenuX = ref(0)
const contextMenuY = ref(0)
const confirmingDelete = ref(false)

const handleEscapeKey = (e) => {
	if (e.key === 'Escape') closeContextMenu()
}

const closeContextMenu = () => {
	if (!contextMenuVisible.value) return
	contextMenuVisible.value = false
	confirmingDelete.value = false
	if (activeCloseContextMenu === closeContextMenu) activeCloseContextMenu = null
	document.removeEventListener('keydown', handleEscapeKey)
}

const showContextMenu = (e) => {
	e.stopPropagation()
	// Close any other open instance context menu first
	activeCloseContextMenu?.()
	confirmingDelete.value = false
	const vw = window.innerWidth
	const vh = window.innerHeight
	const menuW = 180
	const menuH = 130
	contextMenuX.value = e.clientX + menuW > vw ? e.clientX - menuW : e.clientX
	contextMenuY.value = e.clientY + menuH > vh ? e.clientY - menuH : e.clientY
	contextMenuVisible.value = true
	activeCloseContextMenu = closeContextMenu
	document.addEventListener('click', closeContextMenu, { once: true })
	document.addEventListener('contextmenu', closeContextMenu, { once: true })
	document.addEventListener('keydown', handleEscapeKey)
	window.addEventListener('scroll', closeContextMenu, { capture: true, once: true })
}

onUnmounted(() => {
	unlisten()
	closeContextMenu()
})

const openSettings = async () => {
	closeContextMenu()
	await router.push(`/instance/${encodeURIComponent(props.instance.path)}?settings=1`)
}

const deleteInstance = async () => {
	if (!confirmingDelete.value) {
		confirmingDelete.value = true
		return
	}
	closeContextMenu()
	await remove(props.instance.path).catch(handleError)
	emit('deleted', props.instance.path)
}
</script>

<template>
	<template v-if="compact">
		<div
			class="card-shadow grid grid-cols-[auto_1fr_auto] bg-bg-raised rounded-xl p-3 pl-4 gap-2 cursor-pointer hover:brightness-90 transition-all"
			@click="seeInstance"
			@mouseenter="checkProcess"
		>
			<Avatar
				size="48px"
				:src="instance.icon_path ? convertFileSrc(instance.icon_path) : null"
				:tint-by="instance.path"
				alt="Mod card"
			/>
			<div class="h-full flex items-center font-bold text-contrast leading-normal">
				<span class="line-clamp-2">{{ instance.name }}</span>
			</div>
			<div class="flex items-center">
				<ButtonStyled v-if="playing" color="red" circular @mousehover="checkProcess">
					<button v-tooltip="'Stop'" @click="(e) => stop(e, 'InstanceCard')">
						<StopCircleIcon />
					</button>
				</ButtonStyled>
				<ButtonStyled v-else-if="modLoading" color="standard" circular>
					<button v-tooltip="'Instance is loading...'" disabled>
						<SpinnerIcon class="animate-spin" />
					</button>
				</ButtonStyled>
				<ButtonStyled v-else :color="first ? 'brand' : 'standard'" circular>
					<button
						v-tooltip="'Play'"
						@click="(e) => play(e, 'InstanceCard')"
						@mousehover="checkProcess"
					>
						<PlayIcon class="translate-x-[1px]" />
					</button>
				</ButtonStyled>
			</div>
			<div class="flex items-center col-span-3 gap-1 text-secondary font-semibold">
				<TimerIcon />
				<span class="text-sm">
					<template v-if="instance.last_played">
						Played {{ formatRelativeTime(dayjs(instance.last_played).toISOString()) }}
					</template>
					<template v-else> Never played </template>
				</span>
			</div>
		</div>
	</template>
	<div
		v-else
		class="instance-card"
		@click="seeInstance"
		@mouseenter="checkProcess"
		@contextmenu.prevent="showContextMenu"
	>
		<!-- Banner / icon thumbnail -->
		<div class="thumbnail">
			<img v-if="bannerUrl" :src="bannerUrl" :alt="instance.name" class="thumbnail-img" />
			<div v-else class="thumbnail-icon-wrap">
				<img
					v-if="instance.icon_path"
					:src="convertFileSrc(instance.icon_path)"
					:alt="instance.name"
					class="thumbnail-icon"
				/>
				<GameIcon v-else class="fallback-icon" aria-hidden="true" />
			</div>
		</div>

		<!-- Info -->
		<div class="card-info">
			<p class="card-name">{{ instance.name }}</p>
			<p class="card-meta">{{ instance.loader }} {{ instance.game_version }}</p>
		</div>

		<!-- Footer -->
		<div class="card-footer">
			<span v-if="instance.last_played" class="card-time">
				{{ formatRelativeTime(dayjs(instance.last_played).toISOString()) }}
			</span>
			<span v-else />

			<button
				v-if="playing"
				class="action-btn stop-btn"
				aria-label="Stop instance"
				@click="(e) => stop(e, 'InstanceCard')"
			>
				<StopCircleIcon class="btn-icon" />
				Stop
			</button>
			<button
				v-else-if="modLoading || installing"
				class="action-btn play-btn"
				aria-label="Loading…"
				disabled
			>
				<SpinnerIcon class="btn-icon animate-spin" />
			</button>
			<button
				v-else-if="!installed"
				class="action-btn play-btn"
				aria-label="Repair instance"
				@click="(e) => repair(e)"
			>
				<DownloadIcon class="btn-icon" />
				Repair
			</button>
			<button
				v-else
				class="action-btn play-btn"
				aria-label="Play instance"
				@click="(e) => play(e, 'InstanceCard')"
				@mouseenter="checkProcess"
			>
				<PlayIcon class="btn-icon translate-x-[1px]" />
				Play
			</button>
		</div>

		<!-- Right-click context menu -->
		<Teleport to="body">
			<ul
				v-if="contextMenuVisible"
				class="lv-context-menu"
				:style="{ top: contextMenuY + 'px', left: contextMenuX + 'px' }"
				@click.stop
				@contextmenu.prevent
			>
				<li>
					<button class="lv-ctx-item" @click="openSettings">
						<PencilIcon class="lv-ctx-icon" aria-hidden="true" />
						Edit
					</button>
				</li>
				<li>
					<button
						class="lv-ctx-item"
						@click="openFolder(); closeContextMenu()"
					>
						<FolderOpenIcon class="lv-ctx-icon" aria-hidden="true" />
						Open Folder
					</button>
				</li>
				<li class="lv-ctx-separator" role="separator" />
				<li>
					<button
						class="lv-ctx-item lv-ctx-item--danger"
						:class="{ 'lv-ctx-item--confirming': confirmingDelete }"
						@click.stop="deleteInstance"
					>
						<TrashIcon class="lv-ctx-icon" aria-hidden="true" />
						{{ confirmingDelete ? 'Click again to confirm' : 'Delete' }}
					</button>
				</li>
			</ul>
		</Teleport>
	</div>
</template>

<style lang="scss" scoped>
.instance-card {
	background: #221c33;
	border: 1px solid #2a2440;
	border-radius: 16px;
	overflow: hidden;
	cursor: pointer;
	transition: border-color 0.15s;
	display: flex;
	flex-direction: column;

	&:hover {
		border-color: #8e84cf;

		.play-btn:not(:disabled) {
			opacity: 1;
		}
	}
}

/* Thumbnail */

.thumbnail {
	height: 130px;
	background: #1a1728;
	overflow: hidden;
	display: flex;
	align-items: center;
	justify-content: center;
	flex-shrink: 0;
}

.thumbnail-img {
	width: 100%;
	height: 100%;
	object-fit: cover;
	object-position: center top;
}

.thumbnail-icon-wrap {
	display: flex;
	align-items: center;
	justify-content: center;
	width: 100%;
	height: 100%;
}

.thumbnail-icon {
	width: 56px;
	height: 56px;
	object-fit: contain;
	border-radius: 10px;
}

.fallback-icon {
	width: 2rem;
	height: 2rem;
	color: #9892b0;
}

/* Info */

.card-info {
	padding: 10px 12px 6px;
	min-width: 0;
}

.card-name {
	margin: 0;
	font-size: 13px;
	font-weight: 500;
	color: #f3f5ff;
	white-space: nowrap;
	overflow: hidden;
	text-overflow: ellipsis;
	line-height: 1.3;
}

.card-meta {
	margin: 3px 0 0;
	font-size: 11px;
	color: #9892b0;
	text-transform: capitalize;
	line-height: 1.2;
}

/* Footer */

.card-footer {
	display: flex;
	align-items: center;
	justify-content: space-between;
	padding: 8px 12px;
	border-top: 1px solid #2a2440;
	margin-top: auto;
	min-height: 46px;
	gap: 8px;
}

.card-time {
	font-size: 11px;
	color: #9892b0;
	line-height: 1;
	white-space: nowrap;
	overflow: hidden;
	text-overflow: ellipsis;
	min-width: 0;
}

/* Action buttons */

.action-btn {
	display: flex;
	align-items: center;
	gap: 6px;
	padding: 7px 14px;
	border: none;
	border-radius: 8px;
	font-size: 13px;
	font-weight: 600;
	cursor: pointer;
	transition:
		opacity 0.15s,
		background 0.15s;
	flex-shrink: 0;
	line-height: 1;
	white-space: nowrap;
}

.play-btn {
	background: #8e84cf;
	color: #0e0d16;
	opacity: 0;

	&:hover:not(:disabled) {
		background: #a89fe0;
	}

	&:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
}

.stop-btn {
	background: rgba(255, 73, 110, 0.15);
	color: #ff496e;
	opacity: 1;

	&:hover {
		background: rgba(255, 73, 110, 0.25);
	}
}

.btn-icon {
	width: 14px;
	height: 14px;
	flex-shrink: 0;
}

@media (hover: none) {
	.play-btn {
		opacity: 1;
	}
}
</style>

<style lang="scss">
.lv-context-menu {
	position: fixed;
	z-index: 9999;
	background: #1e1b2e;
	border: 1px solid #2a2440;
	border-radius: 10px;
	padding: 4px;
	margin: 0;
	list-style: none;
	min-width: 180px;
	box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.lv-ctx-item {
	display: flex;
	align-items: center;
	gap: 8px;
	width: 100%;
	padding: 7px 10px;
	background: none;
	border: none;
	border-radius: 7px;
	color: #d4cfee;
	font-size: 13px;
	cursor: pointer;
	text-align: left;
	transition: background 0.1s;

	&:hover {
		background: rgba(142, 132, 207, 0.15);
	}

	&--danger {
		color: #ff7a96;

		&:hover {
			background: rgba(255, 73, 110, 0.15);
		}
	}

	&--confirming {
		background: rgba(255, 73, 110, 0.12);
	}
}

.lv-ctx-icon {
	width: 14px;
	height: 14px;
	flex-shrink: 0;
	opacity: 0.8;
}

.lv-ctx-separator {
	height: 1px;
	background: #2a2440;
	margin: 4px 0;
}
</style>
