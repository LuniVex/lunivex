<script setup lang="ts">
import { CheckIcon, DownloadIcon, ExternalIcon, FileIcon, SpinnerIcon } from '@modrinth/assets'
import {
	Avatar,
	Badge,
	Breadcrumbs,
	ButtonStyled,
	Card,
	CopyCode,
	injectNotificationManager,
	useFormatBytes,
	useFormatDateTime,
} from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import CfInstancePicker, { type PickerInstance } from '@/components/ui/CfInstancePicker.vue'
import {
	curseforge_get_file,
	curseforge_get_file_changelog,
	curseforge_get_mod,
	curseforge_get_mods,
	curseforge_install_mod_file,
	curseforge_install_modpack_file,
} from '@/helpers/curseforge.js'
import { get as getInstance } from '@/helpers/profile'
import { releaseColor } from '@/helpers/utils'
import { useBreadcrumbs } from '@/store/breadcrumbs.js'

type CfFile = {
	id: number
	modId: number
	displayName: string
	fileName: string
	gameVersions: string[]
	releaseType: number
	downloadCount: number
	fileDate: string
	fileLength?: number
	dependencies?: { modId: number; relationType: number }[]
	isAvailable: boolean
}

const { handleError } = injectNotificationManager()
const formatDateTime = useFormatDateTime({ timeStyle: 'short', dateStyle: 'long' })
const formatBytes = useFormatBytes()
const route = useRoute()
const router = useRouter()
const breadcrumbs = useBreadcrumbs()

const modId = computed(() => Number(route.params.id))
const fileId = computed(() => Number(route.params.fileId))

const mod = ref<{
	id: number
	name: string
	slug: string
	classId?: number
	logo?: { thumbnailUrl?: string }
	links?: { websiteUrl?: string }
	allowModDistribution?: boolean
} | null>(null)
const file = ref<CfFile | null>(null)
const changelog = ref('')
const loading = ref(true)
const loadError = ref<string | null>(null)

const installing = ref(false)
const installed = ref(false)
const pickerOpen = ref(false)

const dependencies = ref<
	{ modId: number; title: string; icon: string | null; subtitle: string; blocked: boolean }[]
>([])

const isModpack = computed(() => mod.value?.classId === 4471)
const canInstall = computed(() => mod.value?.allowModDistribution !== false)

const cfFileUrl = computed(() =>
	mod.value?.links?.websiteUrl
		? `${mod.value.links.websiteUrl}/files/${fileId.value}`
		: `https://www.curseforge.com/minecraft/mc-mods/${mod.value?.slug ?? ''}/files/${fileId.value}`,
)

const CF_RELATION_LABELS: Record<number, string> = {
	1: 'Embedded',
	2: 'Optional',
	3: 'Required',
	4: 'Tool',
	5: 'Incompatible',
	6: 'Included',
}

function cfChannelType(rt: number): 'release' | 'beta' | 'alpha' {
	return rt === 1 ? 'release' : rt === 2 ? 'beta' : 'alpha'
}

function mcVersions(gv: string[]): string[] {
	return gv.filter((v) => /^\d+\.\d+/.test(v))
}

function loaderNames(gv: string[]): string[] {
	return gv.filter((v) => /^(Forge|Fabric|Quilt|NeoForge|LiteLoader|Cauldron)/i.test(v))
}

onMounted(async () => {
	try {
		const [modData, fileData, changelogHtml] = await Promise.all([
			curseforge_get_mod(modId.value),
			curseforge_get_file(modId.value, fileId.value),
			curseforge_get_file_changelog(modId.value, fileId.value).catch(() => ''),
		])
		mod.value = modData
		file.value = fileData
		changelog.value = changelogHtml
		breadcrumbs.setName('CfProject', modData.name)
		breadcrumbs.setName('CfVersion', fileData.displayName)

		const depIds = (fileData.dependencies ?? []).map((d) => d.modId)
		if (depIds.length) {
			const depMods = await curseforge_get_mods(depIds).catch(() => [])
			dependencies.value = (fileData.dependencies ?? []).map((dep) => {
				const depMod = depMods.find((m: { id: number }) => m.id === dep.modId)
				return {
					modId: dep.modId,
					title: depMod?.name ?? `Project ${dep.modId}`,
					icon: depMod?.logo?.thumbnailUrl ?? null,
					subtitle: CF_RELATION_LABELS[dep.relationType] ?? 'Dependency',
					blocked: !depMod,
				}
			})
		}
	} catch (err) {
		loadError.value = (err as Error)?.message ?? String(err)
	} finally {
		loading.value = false
	}
})

async function handleInstall() {
	if (!mod.value || !file.value || installing.value || installed.value || !canInstall.value) return

	if (isModpack.value) {
		installing.value = true
		try {
			const profilePath = await curseforge_install_modpack_file(mod.value.id, file.value.id)
			installed.value = true
			router.push(`/instance/${encodeURIComponent(profilePath)}`)
		} catch (err) {
			handleError(err as Error)
		} finally {
			installing.value = false
		}
		return
	}

	const instancePath = route.query.i as string | undefined
	if (instancePath) {
		const instance = await getInstance(instancePath).catch(() => null)
		if (instance) {
			await installToPath(instance.path)
			return
		}
	}

	pickerOpen.value = true
}

async function pickerInstall(inst: PickerInstance) {
	pickerOpen.value = false
	await installToPath(inst.path)
}

async function installToPath(profilePath: string) {
	if (!mod.value || !file.value) return
	installing.value = true
	try {
		await curseforge_install_mod_file({
			profilePath,
			modId: mod.value.id,
			fileId: file.value.id,
			fileName: file.value.fileName,
			projectType: 'mod',
		})
		installed.value = true
	} catch (err) {
		handleError(err as Error)
	} finally {
		installing.value = false
	}
}
</script>

<template>
	<div v-if="loading" class="flex items-center justify-center p-16 text-secondary">
		<SpinnerIcon class="animate-spin h-8 w-8" />
	</div>

	<div v-else-if="loadError" class="flex flex-col items-center gap-4 p-16 text-secondary">
		<p class="m-0">Failed to load version: {{ loadError }}</p>
		<ButtonStyled>
			<button @click="router.back()">Go back</button>
		</ButtonStyled>
	</div>

	<div v-else-if="mod && file">
		<Card>
			<Breadcrumbs
				:current-title="file.displayName"
				:link-stack="[
					{
						href: `/cf-project/${mod.id}`,
						label: mod.name,
					},
				]"
			/>
			<div class="version-title">
				<h2>{{ file.displayName }}</h2>
			</div>
			<div class="button-group">
				<ButtonStyled color="brand">
					<button
						v-tooltip="!canInstall ? 'Distribution disabled by author' : null"
						:disabled="installing || installed || !canInstall"
						@click="handleInstall"
					>
						<SpinnerIcon v-if="installing" class="animate-spin" />
						<CheckIcon v-else-if="installed" />
						<DownloadIcon v-else />
						{{
							installing
								? 'Installing...'
								: installed
									? 'Installed'
									: !canInstall
										? 'Distribution disabled'
										: 'Install'
						}}
					</button>
				</ButtonStyled>
				<ButtonStyled>
					<a href="#" rel="external" @click.prevent="openUrl(cfFileUrl)">
						CurseForge website
						<ExternalIcon />
					</a>
				</ButtonStyled>
			</div>
		</Card>
		<div class="version-container">
			<div class="description-cards">
				<Card>
					<h3 class="card-title">Changelog</h3>
					<div v-if="changelog" class="markdown-body" v-html="changelog" />
					<p v-else class="m-0 text-secondary">No changelog provided.</p>
				</Card>
				<Card>
					<h3 class="card-title">Files</h3>
					<Card class="file primary">
						<span class="label">
							<FileIcon />
							<span>
								<span class="title">
									{{ file.fileName }}
								</span>
								<template v-if="file.fileLength"> ({{ formatBytes(file.fileLength) }})</template>
								<span class="primary-label"> Primary </span>
							</span>
						</span>
						<ButtonStyled color="brand">
							<button
								class="download"
								:disabled="installing || installed || !canInstall"
								@click="handleInstall"
							>
								<DownloadIcon v-if="!installed" />
								<CheckIcon v-else />
								{{ installed ? 'Installed' : 'Install' }}
							</button>
						</ButtonStyled>
					</Card>
				</Card>
				<Card v-if="dependencies.length > 0">
					<h2>Dependencies</h2>
					<div v-for="dependency in dependencies" :key="dependency.modId">
						<router-link
							v-if="!dependency.blocked"
							class="btn dependency"
							:to="`/cf-project/${dependency.modId}`"
						>
							<Avatar size="sm" :src="dependency.icon" />
							<div>
								<span class="title"> {{ dependency.title }} </span> <br />
								<span> {{ dependency.subtitle }} </span>
							</div>
						</router-link>
						<div v-else class="dependency disabled" disabled="">
							<Avatar size="sm" :src="dependency.icon" />
							<div class="text">
								<div class="title">{{ dependency.title }}</div>
								<div>{{ dependency.subtitle }}</div>
							</div>
						</div>
					</div>
				</Card>
			</div>
			<Card class="metadata-card">
				<h3 class="card-title">Metadata</h3>
				<div class="metadata">
					<div class="metadata-item">
						<span class="metadata-label">Release Channel</span>
						<span class="metadata-value">
							<Badge
								:color="releaseColor(cfChannelType(file.releaseType))"
								:type="
									cfChannelType(file.releaseType).charAt(0).toUpperCase() +
									cfChannelType(file.releaseType).slice(1)
								"
							/>
						</span>
					</div>
					<div class="metadata-item">
						<span class="metadata-label">File Name</span>
						<span class="metadata-value">{{ file.fileName }}</span>
					</div>
					<div v-if="loaderNames(file.gameVersions).length" class="metadata-item">
						<span class="metadata-label">Loaders</span>
						<span class="metadata-value">{{ loaderNames(file.gameVersions).join(', ') }}</span>
					</div>
					<div v-if="mcVersions(file.gameVersions).length" class="metadata-item">
						<span class="metadata-label">Game Versions</span>
						<span class="metadata-value">{{ mcVersions(file.gameVersions).join(', ') }}</span>
					</div>
					<div class="metadata-item">
						<span class="metadata-label">Downloads</span>
						<span class="metadata-value">{{ Math.round(file.downloadCount) }}</span>
					</div>
					<div class="metadata-item">
						<span class="metadata-label">Publication Date</span>
						<span class="metadata-value">{{ formatDateTime(file.fileDate) }}</span>
					</div>
					<div v-if="file.fileLength" class="metadata-item">
						<span class="metadata-label">File Size</span>
						<span class="metadata-value">{{ formatBytes(file.fileLength) }}</span>
					</div>
					<div class="metadata-item">
						<span class="metadata-label">File ID</span>
						<span class="metadata-value">
							<CopyCode class="copycode" :text="String(file.id)" />
						</span>
					</div>
				</div>
			</Card>
		</div>

		<CfInstancePicker
			:open="pickerOpen"
			:mod-name="mod.name"
			:mod-icon-url="mod.logo?.thumbnailUrl"
			@close="pickerOpen = false"
			@select="pickerInstall"
		/>
	</div>
</template>

<style scoped lang="scss">
.version-container {
	display: flex;
	flex-direction: row;
	gap: 1rem;
}

.version-title {
	margin-bottom: 1rem;
	h2 {
		font-size: var(--font-size-2xl);
		font-weight: 700;
		color: var(--color-contrast);
		margin: 0;
	}
}

.dependency {
	display: flex;
	padding: 0.5rem 1rem 0.5rem 0.5rem;
	gap: 0.5rem;
	background: var(--color-raised-bg);
	color: var(--color-base);
	width: 100%;

	.title {
		font-weight: bolder;
	}

	:deep(svg) {
		margin-right: 0 !important;
	}
}

.file {
	display: flex;
	flex-direction: row;
	gap: 0.5rem;
	background: var(--color-button-bg);
	color: var(--color-base);
	padding: 0.5rem 1rem;

	.download {
		margin-left: auto;
		background-color: var(--color-raised-bg);
	}

	.label {
		display: flex;
		margin: auto 0 auto;
		gap: 0.5rem;

		.title {
			font-weight: bolder;
			word-break: break-all;
		}

		svg {
			min-width: 1.1rem;
			min-height: 1.1rem;
			width: 1.1rem;
			height: 1.1rem;
			margin: auto 0;
		}

		.primary-label {
			font-style: italic;
		}
	}
}

.primary {
	background: var(--color-brand-highlight);
	color: var(--color-contrast);
}

.button-group {
	display: flex;
	flex-wrap: wrap;
	flex-direction: row;
	gap: 0.5rem;
}

.card-title {
	font-size: var(--font-size-lg);
	color: var(--color-contrast);
	margin: 0 0 0.5rem;
}

.description-cards {
	width: 100%;
}

.metadata-card {
	width: 20rem;
	height: min-content;
}

.metadata {
	display: flex;
	flex-direction: column;
	flex-wrap: wrap;
	gap: 1rem;

	.metadata-item {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;

		.metadata-label {
			font-weight: bold;
		}
	}
}

.markdown-body {
	:deep(hr),
	:deep(h1),
	:deep(h2),
	:deep(img) {
		max-width: max(60rem, 90%) !important;
	}

	// CF's raw changelog HTML bakes explicit (often inaccurate) width/height
	// attributes onto every img; forcing width back to auto makes the browser
	// size from true natural dimensions instead, matching Modrinth's own
	// changelogs which carry no such attributes. v-html content isn't
	// scope-attributed, so :deep() is required — a plain nested selector
	// silently fails to match it.
	:deep(img) {
		width: auto !important;
	}

	:deep(ul),
	:deep(ol) {
		margin-left: 2rem;
	}
}

.copycode {
	border: 0;
	color: var(--color-contrast);
}

.disabled {
	display: flex;
	flex-direction: row;
	vertical-align: center;
	align-items: center;
	cursor: not-allowed;
	border-radius: var(--radius-lg);
}
</style>
