<script setup lang="ts">
import { PlusIcon } from '@modrinth/assets'
import { ButtonStyled, injectNotificationManager } from '@modrinth/ui'
import { inject, computed, onUnmounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import Instance from '@/components/ui/Instance.vue'
import RecentWorldsList from '@/components/ui/world/RecentWorldsList.vue'
import { profile_listener } from '@/helpers/events'
import { list } from '@/helpers/profile.js'
import type { GameInstance } from '@/helpers/types'
import { useBreadcrumbs } from '@/store/breadcrumbs'
import dayjs from 'dayjs'

const { handleError } = injectNotificationManager()
const showCreationModal = inject('showCreationModal')
const route = useRoute()
const router = useRouter()
const breadcrumbs = useBreadcrumbs()

breadcrumbs.setRootContext({ name: 'Home', link: route.path })

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

async function fetchInstances() {
	instances.value = await list().catch(handleError)
}

await fetchInstances()

const unlistenProfile = await profile_listener(async () => {
	await fetchInstances()
})

onUnmounted(() => {
	unlistenProfile()
})
</script>

<template>
	<div class="p-6 flex flex-col gap-6">
		<div v-if="recentInstances.length > 0">
			<h1 class="m-0 mb-3 text-2xl font-extrabold">Welcome back!</h1>
			<RecentWorldsList :recent-instances="recentInstances" />
		</div>

		<div>
			<div class="flex items-center justify-between mb-3">
				<h2 class="m-0 text-xl font-bold">
					{{ instances.length > 0 ? 'Your instances' : 'Welcome to LuniVex!' }}
				</h2>
			</div>

			<template v-if="instances.length > 0">
				<section class="instances-grid">
					<Instance
						v-for="instance in instances"
						:key="instance.path + instance.install_stage"
						:instance="instance"
					/>
				</section>
			</template>
			<div v-else class="empty-state">
				<p class="m-0 text-secondary">You don't have any instances yet.</p>
				<div class="flex gap-3 mt-4">
					<ButtonStyled color="brand">
						<button :disabled="offline" @click="showCreationModal?.()">
							<PlusIcon /> Create instance
						</button>
					</ButtonStyled>
					<ButtonStyled>
						<button @click="router.push('/browse/modpack')">Browse modpacks</button>
					</ButtonStyled>
				</div>
			</div>
		</div>

		<p class="m-0">
			<a
				class="text-secondary text-sm cursor-pointer hover:text-primary transition-colors"
				@click="router.push('/browse/modpack')"
			>
				Browse mods and modpacks →
			</a>
		</p>
	</div>
</template>

<style lang="scss" scoped>
.instances-grid {
	display: grid;
	grid-template-columns: repeat(auto-fill, minmax(16rem, 1fr));
	gap: 0.75rem;
	width: 100%;
}

.empty-state {
	display: flex;
	flex-direction: column;
	align-items: flex-start;
	padding: 1.5rem 0;
}
</style>
