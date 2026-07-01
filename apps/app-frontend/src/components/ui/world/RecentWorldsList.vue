<script setup lang="ts">
import { LoaderCircleIcon } from '@modrinth/assets'
import { injectNotificationManager } from '@modrinth/ui'
import { platform } from '@tauri-apps/plugin-os'
import type { Dayjs } from 'dayjs'
import dayjs from 'dayjs'
import { onMounted, onUnmounted, ref, watch } from 'vue'

import InstanceItem from '@/components/ui/world/InstanceItem.vue'
import { process_listener, profile_listener } from '@/helpers/events'
import { get_all } from '@/helpers/process'
import type { GameInstance } from '@/helpers/types'

const { handleError } = injectNotificationManager()

const props = defineProps<{
	recentInstances: GameInstance[]
}>()

const jumpBackInItems = ref<InstanceJumpBackInItem[]>([])
const loading = ref(true)

const MIN_JUMP_BACK_IN = 3
const MAX_JUMP_BACK_IN = 6
const TWO_WEEKS_AGO = dayjs().subtract(14, 'day')

const MAX_LINUX_POPULATES = 3
const isLinux = platform() === 'linux'
const linuxPopulateCount = ref(0)

type InstanceJumpBackInItem = {
	type: 'instance'
	last_played: Dayjs
	instance: GameInstance
}

watch(
	() => props.recentInstances,
	async () => {
		await populateJumpBackIn().catch(() => {
			console.error('Failed to populate jump back in')
		})
	},
)

await populateJumpBackIn()
	.catch(() => {
		console.error('Failed to populate jump back in')
	})
	.finally(() => {
		loading.value = false
	})

async function populateJumpBackIn() {
	if (isLinux && linuxPopulateCount.value >= MAX_LINUX_POPULATES) return
	if (isLinux) linuxPopulateCount.value++

	const items: InstanceJumpBackInItem[] = []
	for (const instance of props.recentInstances) {
		if (!instance.last_played) continue
		items.push({
			type: 'instance',
			last_played: dayjs(instance.last_played),
			instance,
		})
	}

	items.sort((a, b) => b.last_played.diff(a.last_played))
	jumpBackInItems.value = items
		.filter((item, index) => index < MIN_JUMP_BACK_IN || item.last_played.isAfter(TWO_WEEKS_AGO))
		.slice(0, MAX_JUMP_BACK_IN)
}

const currentProfile = ref<string>()

const unlistenProcesses = await process_listener(async () => {
	await checkProcesses()
})

const unlistenProfiles = await profile_listener(async () => {
	await populateJumpBackIn().catch(() => {
		console.error('Failed to populate jump back in')
	})
})

const runningInstances = ref<string[]>([])

type ProcessMetadata = {
	uuid: string
	profile_path: string
	start_time: string
}

const checkProcesses = async () => {
	const runningProcesses: ProcessMetadata[] = await get_all().catch(handleError)

	const runningPaths = runningProcesses.map((x) => x.profile_path)

	const stoppedInstances = runningInstances.value.filter((x) => !runningPaths.includes(x))
	if (currentProfile.value && stoppedInstances.includes(currentProfile.value)) {
		currentProfile.value = undefined
	}

	runningInstances.value = runningPaths
}

onMounted(() => {
	checkProcesses()
	linuxPopulateCount.value = 0
})

onUnmounted(() => {
	unlistenProcesses()
	unlistenProfiles()
})
</script>

<template>
	<div v-if="loading" class="flex flex-col gap-2">
		<h2 class="jump-back-heading">Jump back in</h2>
		<div class="text-center py-4">
			<LoaderCircleIcon class="mx-auto size-8 animate-spin text-contrast" />
		</div>
	</div>
	<div v-else-if="jumpBackInItems.length > 0" class="flex flex-col gap-2">
		<h2 class="jump-back-heading">Jump back in</h2>
		<div class="grid-when-huge flex flex-col w-full gap-2">
			<InstanceItem
				v-for="item in jumpBackInItems"
				:key="item.instance.path"
				:instance="item.instance"
				:last_played="item.last_played"
			/>
		</div>
	</div>
</template>
<style scoped lang="scss">
.jump-back-heading {
	margin: 0 0 0.625rem;
	font-size: 1.25rem;
	font-weight: 700;
	color: #f3f5ff;
	border-left: 3px solid #8e84cf;
	padding-left: 10px;
}

.grid-when-huge {
	display: grid;
	grid-template-columns: repeat(auto-fill, minmax(670px, 1fr));
}
</style>
