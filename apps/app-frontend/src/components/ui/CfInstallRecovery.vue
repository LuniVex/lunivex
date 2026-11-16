<script setup lang="ts">
import { DownloadIcon, ExternalIcon, RefreshCwIcon, SpinnerIcon } from '@modrinth/assets'
import { ButtonStyled, injectNotificationManager, NewModal } from '@modrinth/ui'
import { open } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import {
	curseforge_discard_install,
	curseforge_import_install_files,
	curseforge_pending_installs,
	curseforge_resume_install,
	onCurseForgeInstallPaused,
} from '@/helpers/curseforge.js'

type PendingInstall = {
	id: string
	modId: number
	fileId: number
	name: string
	downloadedFiles: number
	totalFiles: number
	missingFiles: { modId: number; fileId: number; fileName: string; url: string | null }[]
	error: string | null
}
type InstallResult = { profilePath: string | null; pending: PendingInstall | null }
type ImportResult = { imported: number; rejected: string[]; pending: PendingInstall }

const { handleError, addNotification } = injectNotificationManager()
const router = useRouter()
const modal = ref<InstanceType<typeof NewModal>>()
const jobs = ref<PendingInstall[]>([])
const selectedId = ref('')
const busy = ref(false)
const visible = ref(false)
const importErrors = ref<string[]>([])
const selected = computed(() => jobs.value.find((job) => job.id === selectedId.value))

function setJob(job: PendingInstall) {
	jobs.value = [job, ...jobs.value.filter((existing) => existing.id !== job.id)]
	selectedId.value = job.id
}

function show(job?: PendingInstall) {
	if (job) setJob(job)
	if (!selected.value) selectedId.value = jobs.value[0]?.id ?? ''
	importErrors.value = []
	visible.value = true
	modal.value?.show()
}

async function refresh(showSaved = false) {
	try {
		jobs.value = (await curseforge_pending_installs()) as PendingInstall[]
		if (!selected.value) selectedId.value = jobs.value[0]?.id ?? ''
		if (showSaved && jobs.value.length) show()
	} catch (error) {
		handleError(error as Error)
	}
}

const unsubscribe = onCurseForgeInstallPaused((job: PendingInstall) => show(job))
onUnmounted(unsubscribe)
onMounted(() => refresh(true))

async function resume() {
	const job = selected.value
	if (!job || busy.value) return
	busy.value = true
	importErrors.value = []
	try {
		const result = (await curseforge_resume_install(job.id)) as InstallResult
		if (result.pending) {
			setJob(result.pending)
		} else if (result.profilePath) {
			jobs.value = jobs.value.filter((existing) => existing.id !== job.id)
			if (visible.value) {
				modal.value?.hide()
				await router.push('/instance/' + encodeURIComponent(result.profilePath))
			} else {
				addNotification({ title: 'Modpack installed', text: job.name, type: 'success' })
			}
		} else {
			throw new Error('CurseForge returned an invalid installation result')
		}
	} catch (error) {
		handleError(error as Error)
	} finally {
		busy.value = false
	}
}

async function importFiles() {
	const job = selected.value
	if (!job || busy.value) return
	busy.value = true
	try {
		const paths = await open({
			multiple: true,
			title: 'Select the exact CurseForge files you downloaded',
			filters: [{ name: 'Minecraft files', extensions: ['jar', 'zip'] }],
		})
		if (!paths) return
		const result = (await curseforge_import_install_files(
			job.id,
			Array.isArray(paths) ? paths : [paths],
		)) as ImportResult
		setJob(result.pending)
		importErrors.value = result.rejected
	} catch (error) {
		handleError(error as Error)
	} finally {
		busy.value = false
	}
}

async function discard() {
	const job = selected.value
	if (!job || busy.value) return
	busy.value = true
	try {
		await curseforge_discard_install(job.id)
		jobs.value = jobs.value.filter((existing) => existing.id !== job.id)
		selectedId.value = jobs.value[0]?.id ?? ''
		importErrors.value = []
		if (!jobs.value.length) modal.value?.hide()
	} catch (error) {
		handleError(error as Error)
	} finally {
		busy.value = false
	}
}

async function openFile(url: string) {
	try {
		await openUrl(url)
	} catch (error) {
		handleError(error as Error)
	}
}
</script>

<template>
	<div v-if="jobs.length && !visible" class="fixed bottom-4 left-1/2 z-40 -translate-x-1/2">
		<ButtonStyled color="brand">
			<button @click="show()">
				<DownloadIcon />
				Saved installs ({{ jobs.length }})
			</button>
		</ButtonStyled>
	</div>
	<NewModal
		ref="modal"
		header="Finish CurseForge installation"
		max-width="720px"
		:on-hide="() => (visible = false)"
	>
		<div class="flex flex-col gap-4">
			<label v-if="jobs.length > 1" class="flex flex-col gap-2">
				Saved installation
				<select v-model="selectedId" :disabled="busy" class="rounded-lg bg-bg-raised p-2">
					<option v-for="job in jobs" :key="job.id" :value="job.id">{{ job.name }}</option>
				</select>
			</label>
			<template v-if="selected">
				<div>
					<h3 class="m-0">{{ selected.name }}</h3>
					<p class="mb-0 text-secondary">
						{{ selected.downloadedFiles }} of {{ selected.totalFiles }} required files saved. You
						can close this window and continue later.
					</p>
				</div>
				<p v-if="selected.error" class="m-0 rounded-lg bg-bg-raised p-3" role="alert">
					Could not continue: {{ selected.error }}
				</p>
				<template v-if="selected.missingFiles.length">
					<p class="m-0">
						These files need a manual download from CurseForge. Open each official page, download
						the listed version, then select the downloaded files here.
					</p>
					<ul class="m-0 flex max-h-64 list-none flex-col gap-3 overflow-y-auto p-0">
						<li
							v-for="file in selected.missingFiles"
							:key="file.modId + '-' + file.fileId"
							class="flex items-center justify-between gap-3 rounded-lg bg-bg-raised p-3"
						>
							<span class="min-w-0 break-all">{{ file.fileName }}</span>
							<ButtonStyled v-if="file.url" type="outlined" size="small">
								<button @click="openFile(file.url!)"><ExternalIcon /> Open page</button>
							</ButtonStyled>
							<span v-else class="text-secondary"
								>Project {{ file.modId }}, file {{ file.fileId }}</span
							>
						</li>
					</ul>
					<ButtonStyled color="brand">
						<button :disabled="busy" @click="importFiles">
							<DownloadIcon /> Select downloaded files
						</button>
					</ButtonStyled>
					<p class="m-0 text-sm text-secondary">
						Files are checked against CurseForge’s checksums. Only matching versions are accepted.
					</p>
				</template>
				<p v-else class="m-0">Your saved downloads are ready to resume.</p>
				<ul v-if="importErrors.length" class="m-0" role="alert">
					<li v-for="error in importErrors" :key="error">{{ error }}</li>
				</ul>
			</template>
		</div>
		<template #actions>
			<div class="flex flex-wrap justify-between gap-2">
				<ButtonStyled type="outlined" color="red">
					<button :disabled="busy || !selected" @click="discard">Discard saved downloads</button>
				</ButtonStyled>
				<div class="flex gap-2">
					<ButtonStyled type="outlined">
						<button @click="modal?.hide()">Close</button>
					</ButtonStyled>
					<ButtonStyled color="brand">
						<button :disabled="busy || !selected || !!selected.missingFiles.length" @click="resume">
							<SpinnerIcon v-if="busy" class="animate-spin" />
							<RefreshCwIcon v-else />
							{{ busy ? 'Continuing…' : 'Resume installation' }}
						</button>
					</ButtonStyled>
				</div>
			</div>
		</template>
	</NewModal>
</template>
