<script setup lang="ts">
import {
	EyeIcon,
	FolderOpenIcon,
	MoreVerticalIcon,
	PlayIcon,
	SpinnerIcon,
	StopCircleIcon,
} from '@modrinth/assets'
import {
	Avatar,
	ButtonStyled,
	commonMessages,
	injectNotificationManager,
	OverflowMenu,
	SmartClickable,
	useFormatDateTime,
	useRelativeTime,
	useVIntl,
} from '@modrinth/ui'
import { capitalizeString } from '@modrinth/utils'
import { convertFileSrc } from '@tauri-apps/api/core'
import type { Dayjs } from 'dayjs'
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { trackEvent } from '@/helpers/analytics'
import { get_project } from '@/helpers/cache'
import { process_listener } from '@/helpers/events'
import { get_by_profile_path } from '@/helpers/process'
import { kill, run } from '@/helpers/profile'
import type { GameInstance } from '@/helpers/types'
import { showProfileInFolder } from '@/helpers/utils'
import { handleSevereError } from '@/store/error'

const { handleError } = injectNotificationManager()
const { formatMessage } = useVIntl()
const formatRelativeTime = useRelativeTime()
const formatDateTime = useFormatDateTime({
	timeStyle: 'short',
	dateStyle: 'long',
})

const router = useRouter()

const emit = defineEmits<{
	(e: 'play' | 'stop'): void
}>()

const props = defineProps<{
	instance: GameInstance
	last_played: Dayjs
}>()

const loadingModpack = ref(!!props.instance.linked_data)

const modpack = ref()

if (props.instance.linked_data) {
	nextTick().then(async () => {
		modpack.value = await get_project(props.instance.linked_data?.project_id, 'must_revalidate')
		loadingModpack.value = false
	})
}

const instanceIcon = computed(() => props.instance.icon_path)

const loader = computed(() => {
	if (props.instance.loader === 'vanilla') {
		return 'Minecraft'
	} else if (props.instance.loader === 'neoforge') {
		return 'NeoForge'
	} else {
		return capitalizeString(props.instance.loader)
	}
})

const loading = ref(false)
const playing = ref(false)

const play = async (event: MouseEvent) => {
	event?.stopPropagation()
	loading.value = true
	await run(props.instance.path)
		.catch((err) => handleSevereError(err, { profilePath: props.instance.path }))
		.finally(() => {
			trackEvent('InstanceStart', {
				loader: props.instance.loader,
				game_version: props.instance.game_version,
				source: 'InstanceItem',
			})
		})
	emit('play')
	loading.value = false
}

const stop = async (event: MouseEvent) => {
	event?.stopPropagation()
	loading.value = true
	await kill(props.instance.path).catch(handleError)
	trackEvent('InstanceStop', {
		loader: props.instance.loader,
		game_version: props.instance.game_version,
		source: 'InstanceItem',
	})
	emit('stop')
	loading.value = false
}

const unlistenProcesses = await process_listener(async () => {
	await checkProcess()
})

const checkProcess = async () => {
	const runningProcesses = await get_by_profile_path(props.instance.path).catch(handleError)

	playing.value = runningProcesses.length > 0
}

onMounted(() => {
	checkProcess()
})

onUnmounted(() => {
	unlistenProcesses()
})
</script>
<template>
	<SmartClickable>
		<template #clickable>
			<router-link
				class="no-click-animation"
				:to="`/instance/${encodeURIComponent(instance.path)}`"
			/>
		</template>
		<div
			class="lv-item-card grid grid-cols-[auto_minmax(0,3fr)_minmax(0,4fr)_auto] items-center gap-2 p-3 rounded-2xl smart-clickable:highlight-on-hover"
		>
			<Avatar
				:src="instanceIcon ? convertFileSrc(instanceIcon) : undefined"
				:tint-by="instance.path"
				size="48px"
			/>
			<div class="flex flex-col col-span-2 justify-between h-full">
				<div class="flex items-center gap-2">
					<div class="lv-name text-lg font-bold truncate smart-clickable:underline-on-hover">
						{{ instance.name }}
					</div>
				</div>
				<div class="flex items-center gap-2 text-sm lv-meta">
					<div
						v-tooltip="instance.last_played ? formatDateTime(instance.last_played) : null"
						class="w-fit shrink-0"
						:class="{ 'cursor-help smart-clickable:allow-pointer-events': last_played }"
					>
						<template v-if="last_played">
							{{
								formatMessage(commonMessages.playedLabel, {
									ago: formatRelativeTime(last_played.toISOString?.()),
								})
							}}
						</template>
						<template v-else> Not played yet </template>
					</div>
					•
					<span v-if="modpack" class="flex items-center gap-1 truncate">
						<router-link
							class="inline-flex items-center gap-1 truncate hover:underline lv-meta smart-clickable:allow-pointer-events"
							:to="`/project/${modpack.id}`"
						>
							<Avatar :src="modpack.icon_url" size="16px" class="shrink-0" />
							<span class="truncate">{{ modpack.title }}</span>
						</router-link>
						({{ loader }} {{ instance.game_version }})
					</span>
					<span v-else-if="loadingModpack" class="flex items-center gap-1 truncate">
						<SpinnerIcon class="animate-spin shrink-0" />
						<span class="truncate">Loading modpack...</span>
					</span>
					<span v-else class="flex items-center gap-1 truncate">
						{{ loader }}
						{{ instance.game_version }}
					</span>
				</div>
			</div>
			<div class="flex gap-1 justify-end smart-clickable:allow-pointer-events">
				<ButtonStyled v-if="playing && !loading" color="red">
					<button @click="stop">
						<StopCircleIcon aria-hidden="true" />
						{{ formatMessage(commonMessages.stopButton) }}
					</button>
				</ButtonStyled>
				<button
					v-else
					v-tooltip="playing ? 'Instance is already open' : null"
					class="lv-play-btn"
					:disabled="playing || loading"
					@click="play"
				>
					<SpinnerIcon v-if="loading" class="animate-spin lv-btn-icon" aria-hidden="true" />
					<PlayIcon v-else class="lv-btn-icon translate-x-[1px]" aria-hidden="true" />
					{{ formatMessage(commonMessages.playButton) }}
				</button>
				<ButtonStyled circular type="transparent">
					<OverflowMenu
						:options="[
							{
								id: 'open-instance',
								shown: !!instance.path,
								action: () => router.push(encodeURI(`/instance/${instance.path}`)),
							},
							{
								id: 'open-folder',
								action: () => showProfileInFolder(instance.path),
							},
						]"
					>
						<MoreVerticalIcon aria-hidden="true" />
						<template #open-instance>
							<EyeIcon aria-hidden="true" />
							View instance
						</template>
						<template #open-folder>
							<FolderOpenIcon aria-hidden="true" />
							{{ formatMessage(commonMessages.openFolderButton) }}
						</template>
					</OverflowMenu>
				</ButtonStyled>
			</div>
		</div>
	</SmartClickable>
</template>

<style lang="scss" scoped>
.lv-item-card {
	background: #221c33;
	border: 1px solid #2a2440;
	border-radius: 16px;
	transition: border-color 0.15s;

	&:hover {
		border-color: #8e84cf;
	}
}

.lv-name {
	color: #f3f5ff;
}

.lv-meta {
	color: #9892b0;
}

.lv-play-btn {
	display: flex;
	align-items: center;
	gap: 6px;
	padding: 7px 14px;
	background: #8e84cf;
	color: #0e0d16;
	border: none;
	border-radius: 8px;
	font-size: 13px;
	font-weight: 600;
	cursor: pointer;
	transition: background 0.15s;
	white-space: nowrap;

	&:hover:not(:disabled) {
		background: #a89fe0;
	}

	&:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
}

.lv-btn-icon {
	width: 14px;
	height: 14px;
	flex-shrink: 0;
}
</style>
