<script setup lang="ts">
import {
	CalendarIcon,
	CheckIcon,
	ClientIcon,
	ClipboardCopyIcon,
	CodeIcon,
	ContractIcon,
	DownloadIcon,
	ExpandIcon,
	ExternalIcon,
	getLoaderIcon,
	GlobeIcon,
	HeartIcon,
	IssuesIcon,
	LeftArrowIcon,
	MonitorSmartphoneIcon,
	MoreVerticalIcon,
	PlusIcon,
	ReportIcon,
	RightArrowIcon,
	ServerIcon,
	SpinnerIcon,
	VersionIcon,
	WikiIcon,
	XIcon,
} from '@modrinth/assets'
import {
	AutoLink,
	Avatar,
	ButtonStyled,
	Card,
	ContentPageHeader,
	DropdownSelect,
	FormattedTag,
	injectNotificationManager,
	NavTabs,
	OverflowMenu,
	Pagination,
	TagItem,
	useCompactNumber,
	useFormatDateTime,
	useRelativeTime,
	useVIntl,
	VersionChannelIndicator,
} from '@modrinth/ui'
import { formatVersionsForDisplay } from '@modrinth/utils'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import CfInstancePicker from '@/components/ui/CfInstancePicker.vue'
import ContextMenu from '@/components/ui/ContextMenu.vue'
import InstanceIndicator from '@/components/ui/InstanceIndicator.vue'
import {
	curseforge_get_mod,
	curseforge_get_mod_description,
	curseforge_get_mod_files,
	curseforge_install_mod,
	curseforge_install_mod_file,
	curseforge_install_modpack,
	curseforge_install_modpack_file,
} from '@/helpers/curseforge.js'
import { get as getInstance, get_projects as getInstanceProjects } from '@/helpers/profile'
import { get_game_versions } from '@/helpers/tags.js'
import { useBreadcrumbs } from '@/store/breadcrumbs.js'

useVIntl()
const { handleError } = injectNotificationManager()
const { formatCompactNumber } = useCompactNumber()
const formatRelativeTime = useRelativeTime()
const formatDateTime = useFormatDateTime({ timeStyle: 'short', dateStyle: 'long' })
const route = useRoute()
const router = useRouter()
const breadcrumbs = useBreadcrumbs()

// ── Types ─────────────────────────────────────────────────────────────────────

type CfMod = {
	id: number
	name: string
	slug: string
	summary: string
	downloadCount: number
	thumbsUpCount?: number
	classId?: number
	categories: { id: number; name: string; slug: string }[]
	authors: { id: number; name: string; url?: string }[]
	logo?: { thumbnailUrl?: string }
	links?: { websiteUrl?: string; wikiUrl?: string; issuesUrl?: string; sourceUrl?: string }
	screenshots?: { id?: number; title?: string; description?: string; thumbnailUrl?: string; url?: string }[]
	latestFiles?: CfFile[]
	latestFilesIndexes: { gameVersion: string; fileId: number; filename: string; releaseType: number; modLoader?: number }[]
	dateCreated?: string
	dateModified: string
	gamePopularityRank?: number
	isAvailable: boolean
	allowModDistribution?: boolean
}

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
	downloadUrl?: string
	dependencies?: { modId: number; relationType: number }[]
	isAvailable: boolean
}

type PickerInstance = {
	path: string
	name: string
	loader: string
	gameVersion: string
	iconPath?: string
}

// ── Constants ─────────────────────────────────────────────────────────────────

const cfBetaTooltip =
	"CurseForge integration is new - some pages may not match Modrinth's layout exactly yet, and bugs are possible. Report issues on Discord."

const CF_LOADER_NAMES: Record<number, string> = {
	1: 'Forge',
	2: 'Cauldron',
	3: 'LiteLoader',
	4: 'Fabric',
	5: 'Quilt',
	6: 'NeoForge',
}

// ── State ─────────────────────────────────────────────────────────────────────

const mod = ref<CfMod | null>(null)
const description = ref('')
const loading = ref(true)
const loadError = ref<string | null>(null)

// A dedicated sample of files (independent of the Versions tab's own
// paginated/filtered `files`) used only to derive environment tags. `mod.
// latestFiles` is capped at ~4 entries and often doesn't include any file
// tagged Client/Server, silently hiding the sidebar section for most mods.
const envSampleFiles = ref<CfFile[]>([])

const installing = ref(false)
const installed = ref(false)

const instance = ref<{ path: string; name: string; game_version: string; loader: string } | null>(null)
const instanceFull = ref<{
	path: string
	name: string
	game_version: string
	loader: string
	install_stage: string
	icon_path?: string
} | null>(null)
const instanceFileNames = ref(new Set<string>())

const FILES_PAGE_SIZE = 20

const files = ref<CfFile[]>([])
const filesLoading = ref(false)
const filesLoaded = ref(false)
const filesPage = ref(1)
const filesTotalCount = ref(0)
const filterGameVersion = ref<string | null>(null)
const filterLoader = ref<string | null>(null)
const filterChannel = ref<string | null>(null)

const gameVersionTags = ref<{ version: string; version_type: string; date: string }[]>([])

const activeTab = ref<'description' | 'versions' | 'gallery'>('description')

// Gallery expand state
const expandedImage = ref<{ title?: string; description?: string; url?: string; thumbnailUrl?: string } | null>(null)
const expandedIndex = ref(0)
const zoomedIn = ref(false)

// Instance picker state
const pickerOpen = ref(false)
const installingFileIds = ref(new Set<number>())
const pickerFileId = ref<number | null>(null)
const pickerFileName = ref('')

// ── Computed ──────────────────────────────────────────────────────────────────

const modId = computed(() => Number(route.params.id))
const isModpack = computed(() => mod.value?.classId === 4471)
const canInstall = computed(() => mod.value?.allowModDistribution !== false)

const screenshots = computed(() =>
	(mod.value?.screenshots ?? []).filter((s) => s.thumbnailUrl || s.url),
)

const supportedGameVersions = computed(() => {
	if (!mod.value) return []
	const seen = new Set<string>()
	const out: string[] = []
	for (const f of mod.value.latestFilesIndexes) {
		if (!seen.has(f.gameVersion)) {
			seen.add(f.gameVersion)
			out.push(f.gameVersion)
		}
	}
	return out.sort((a, b) => {
		const pa = a.split('.').map(Number)
		const pb = b.split('.').map(Number)
		for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
			const d = (pb[i] ?? 0) - (pa[i] ?? 0)
			if (d !== 0) return d
		}
		return 0
	})
})

const supportedLoaders = computed(() => {
	if (!mod.value) return []
	const seen = new Set<number>()
	const out: string[] = []
	for (const f of mod.value.latestFilesIndexes) {
		if (f.modLoader != null && !seen.has(f.modLoader)) {
			seen.add(f.modLoader)
			const name = CF_LOADER_NAMES[f.modLoader]
			if (name) out.push(name)
		}
	}
	return out
})

const displayGameVersions = computed(() =>
	gameVersionTags.value.length
		? formatVersionsForDisplay(supportedGameVersions.value, gameVersionTags.value)
		: supportedGameVersions.value,
)

const environmentTags = computed(() => {
	let client = false
	let server = false
	for (const f of envSampleFiles.value) {
		if (f.gameVersions?.includes('Client')) client = true
		if (f.gameVersions?.includes('Server')) server = true
	}
	if (client && server) return [{ icon: MonitorSmartphoneIcon, label: 'Client and server' }]
	if (client) return [{ icon: ClientIcon, label: 'Client-side' }]
	if (server) return [{ icon: ServerIcon, label: 'Server-side' }]
	return []
})

const cfTypeSlug = computed(() => {
	switch (mod.value?.classId) {
		case 4471:
			return 'modpack'
		case 12:
			return 'resourcepack'
		case 6552:
			return 'shader'
		default:
			return 'mod'
	}
})

const cfWebsiteUrl = computed(
	() =>
		mod.value?.links?.websiteUrl ??
		`https://www.curseforge.com/minecraft/mc-mods/${mod.value?.slug ?? ''}`,
)

const installedInInstance = computed(() => {
	if (!instance.value || instanceFileNames.value.size === 0 || !mod.value) return false
	return (
		mod.value.latestFilesIndexes.some((idx) => instanceFileNames.value.has(idx.filename)) ||
		files.value.some((f) => instanceFileNames.value.has(f.fileName))
	)
})

const tabLinks = computed(() => [
	{ label: 'Description', href: '' },
	{ label: 'Versions', href: '' },
	{ label: 'Gallery', href: '', shown: screenshots.value.length > 0 },
])

const activeTabIndex = computed(() => {
	const filtered = tabLinks.value.filter((l) => l.shown !== false)
	const label =
		activeTab.value === 'description'
			? 'Description'
			: activeTab.value === 'versions'
				? 'Versions'
				: 'Gallery'
	const idx = filtered.findIndex((l) => l.label === label)
	return idx === -1 ? 0 : idx
})

const sortedFiles = computed(() =>
	[...files.value].sort(
		(a, b) => new Date(b.fileDate).getTime() - new Date(a.fileDate).getTime(),
	),
)

const displayedFiles = computed(() =>
	filterChannel.value
		? sortedFiles.value.filter((f) => cfChannelType(f.releaseType) === filterChannel.value)
		: sortedFiles.value,
)

const filesPageCount = computed(() =>
	Math.max(1, Math.ceil(filesTotalCount.value / FILES_PAGE_SIZE)),
)

const filterLoaderOptions = computed(() =>
	supportedLoaders.value.filter((l) => ['Forge', 'Fabric', 'Quilt', 'NeoForge'].includes(l)),
)

// ── Lifecycle ─────────────────────────────────────────────────────────────────

// Vue Router reuses this component instance across navigations between two
// /cf-project/:id routes (same route record), so all fetching/reset logic
// must be callable again on param change, not just once in onMounted —
// mirroring project/Index.vue's fetchProjectData + route.params.id watcher.
async function fetchModData() {
	loading.value = true
	loadError.value = null
	mod.value = null
	description.value = ''
	files.value = []
	filesLoaded.value = false
	filesPage.value = 1
	filesTotalCount.value = 0
	filterGameVersion.value = null
	filterLoader.value = null
	filterChannel.value = null
	activeTab.value = 'description'
	installed.value = false
	instance.value = null
	instanceFull.value = null
	instanceFileNames.value = new Set()
	envSampleFiles.value = []

	curseforge_get_mod_files(modId.value, undefined, undefined, 0, 50)
		.then((r) => {
			envSampleFiles.value = r.data
		})
		.catch(() => {})

	const instancePath = route.query.i as string | undefined
	if (instancePath) {
		const loaded = await getInstance(instancePath).catch(() => null)
		if (loaded) {
			instance.value = {
				path: loaded.path,
				name: loaded.name,
				game_version: loaded.game_version,
				loader: loaded.loader,
			}
			instanceFull.value = loaded
			getInstanceProjects(instancePath)
				.then((projects: Record<string, unknown>) => {
					instanceFileNames.value = new Set(
						Object.keys(projects ?? {}).map((p) => p.split('/').pop() ?? p),
					)
				})
				.catch(() => {})
		}
	}

	try {
		const [modData, desc] = await Promise.all([
			curseforge_get_mod(modId.value),
			curseforge_get_mod_description(modId.value).catch(() => ''),
		])
		mod.value = modData
		description.value = desc
		breadcrumbs.setName('CfProject', modData.name)
	} catch (err) {
		loadError.value = (err as Error)?.message ?? String(err)
	} finally {
		loading.value = false
	}
}

onMounted(async () => {
	get_game_versions()
		.then((tags) => {
			gameVersionTags.value = tags ?? []
		})
		.catch(() => {})

	await fetchModData()
})

watch(
	() => route.params.id,
	async () => {
		await fetchModData()
	},
)

// Gallery keyboard navigation
function keyListener(e: KeyboardEvent) {
	if (!expandedImage.value) return
	if (e.key === 'Escape') {
		e.preventDefault()
		closeExpand()
	} else if (e.key === 'ArrowLeft') {
		e.preventDefault()
		prevImage()
	} else if (e.key === 'ArrowRight') {
		e.preventDefault()
		nextImage()
	}
}

onMounted(() => document.addEventListener('keydown', keyListener))
onUnmounted(() => document.removeEventListener('keydown', keyListener))

// ── Tab switching ─────────────────────────────────────────────────────────────

function onTabClick(_index: number, link: { label: string }) {
	if (link.label === 'Description') {
		activeTab.value = 'description'
	} else if (link.label === 'Versions') {
		activeTab.value = 'versions'
		if (!filesLoaded.value) loadFiles(1)
	} else if (link.label === 'Gallery') {
		activeTab.value = 'gallery'
	}
}

// ── File loading ──────────────────────────────────────────────────────────────

async function loadFiles(page = filesPage.value) {
	if (filesLoading.value || !mod.value) return
	filesLoading.value = true
	try {
		const result = await curseforge_get_mod_files(
			mod.value.id,
			filterGameVersion.value ?? undefined,
			filterLoader.value ?? undefined,
			page - 1,
			FILES_PAGE_SIZE,
		)
		files.value = result.data
		filesTotalCount.value = result.pagination.totalCount
		filesPage.value = page
		filesLoaded.value = true
	} catch (err) {
		handleError(err as Error)
	} finally {
		filesLoading.value = false
	}
}

watch([filterGameVersion, filterLoader], () => {
	if (filesLoaded.value) loadFiles(1)
})

function switchFilesPage(page: number) {
	loadFiles(page)
	document.querySelector('.app-viewport')?.scrollTo({ top: 0, behavior: 'smooth' })
}

// ── Install handlers ──────────────────────────────────────────────────────────

async function handleInstall() {
	if (!mod.value || installing.value || !canInstall.value) return

	if (isModpack.value) {
		installing.value = true
		try {
			const profilePath = await curseforge_install_modpack(mod.value.id)
			installed.value = true
			router.push(`/instance/${encodeURIComponent(profilePath)}`)
		} catch (err) {
			handleError(err as Error)
		} finally {
			installing.value = false
		}
		return
	}

	if (instance.value) {
		installing.value = true
		try {
			await curseforge_install_mod({
				profilePath: instance.value.path,
				modId: mod.value.id,
				gameVersion: instance.value.game_version,
				loader: instance.value.loader,
				projectType: 'mod',
			})
			installed.value = true
		} catch (err) {
			handleError(err as Error)
		} finally {
			installing.value = false
		}
		return
	}

	// open instance picker (best-file selection, not file-specific)
	pickerFileId.value = null
	pickerFileName.value = ''
	pickerOpen.value = true
}

async function pickerInstall(inst: PickerInstance) {
	if (!mod.value) return
	pickerOpen.value = false

	if (pickerFileId.value !== null) {
		const fileId = pickerFileId.value
		const fileName = pickerFileName.value
		pickerFileId.value = null
		installingFileIds.value = new Set([...installingFileIds.value, fileId])
		try {
			await curseforge_install_mod_file({
				profilePath: inst.path,
				modId: mod.value.id,
				fileId,
				fileName,
				projectType: 'mod',
			})
		} catch (err) {
			handleError(err as Error)
		} finally {
			installingFileIds.value = new Set([...installingFileIds.value].filter((id) => id !== fileId))
		}
		return
	}

	installing.value = true
	try {
		await curseforge_install_mod({
			profilePath: inst.path,
			modId: mod.value.id,
			gameVersion: inst.gameVersion,
			loader: inst.loader,
			projectType: 'mod',
		})
		installed.value = true
	} catch (err) {
		handleError(err as Error)
	} finally {
		installing.value = false
	}
}

async function handleVersionInstall(file: CfFile) {
	if (!mod.value || !canInstall.value) return

	if (isModpack.value) {
		installingFileIds.value = new Set([...installingFileIds.value, file.id])
		try {
			const profilePath = await curseforge_install_modpack_file(mod.value.id, file.id)
			router.push(`/instance/${encodeURIComponent(profilePath)}`)
		} catch (err) {
			handleError(err as Error)
		} finally {
			installingFileIds.value = new Set([...installingFileIds.value].filter((id) => id !== file.id))
		}
		return
	}

	const target = instance.value
	if (target) {
		installingFileIds.value = new Set([...installingFileIds.value, file.id])
		try {
			await curseforge_install_mod_file({
				profilePath: target.path,
				modId: mod.value.id,
				fileId: file.id,
				fileName: file.fileName,
				projectType: 'mod',
			})
		} catch (err) {
			handleError(err as Error)
		} finally {
			installingFileIds.value = new Set([...installingFileIds.value].filter((id) => id !== file.id))
		}
		return
	}

	// No instance context — open picker with this specific file
	pickerFileId.value = file.id
	pickerFileName.value = file.fileName
	pickerOpen.value = true
}

// ── Gallery handlers ──────────────────────────────────────────────────────────

function expandImage(img: (typeof screenshots.value)[number], index: number) {
	expandedImage.value = img
	expandedIndex.value = index
	zoomedIn.value = false
}

function closeExpand() {
	expandedImage.value = null
}

function nextImage() {
	expandedIndex.value = (expandedIndex.value + 1) % screenshots.value.length
	expandedImage.value = screenshots.value[expandedIndex.value]
}

function prevImage() {
	expandedIndex.value =
		(expandedIndex.value - 1 + screenshots.value.length) % screenshots.value.length
	expandedImage.value = screenshots.value[expandedIndex.value]
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function cfChannelType(rt: number): 'release' | 'beta' | 'alpha' {
	return rt === 1 ? 'release' : rt === 2 ? 'beta' : 'alpha'
}

function authorInitial(name: string): string {
	return (name.trim()[0] ?? '?').toUpperCase()
}

function authorUrl(author: { name: string; url?: string }): string {
	return author.url ?? `https://www.curseforge.com/members/${encodeURIComponent(author.name)}/projects`
}

function goToCategory(cat: { id: number }) {
	router.push(`/browse/${cfTypeSlug.value}?src=curseforge&cfcat=${cat.id}`)
}

function versionHref(fileId: number): string {
	const params = new URLSearchParams()
	for (const [key, val] of Object.entries(route.query)) {
		if (Array.isArray(val)) {
			for (const v of val) if (v) params.append(key, String(v))
		} else if (val) {
			params.append(key, String(val))
		}
	}
	const qs = params.toString()
	const base = `/cf-project/${mod.value?.id}/version/${fileId}`
	return qs ? `${base}?${qs}` : base
}

const options = ref<InstanceType<typeof ContextMenu> | null>(null)
function handleRightClick(event: MouseEvent) {
	options.value?.showMenu(event, mod.value, [
		{ name: 'install' },
		{ type: 'divider' },
		{ name: 'open_link' },
		{ name: 'copy_link' },
	])
}
function handleOptionsClick(args: { option: string }) {
	switch (args.option) {
		case 'install':
			handleInstall()
			break
		case 'open_link':
			openUrl(cfWebsiteUrl.value)
			break
		case 'copy_link':
			navigator.clipboard.writeText(cfWebsiteUrl.value)
			break
	}
}

function mcVersions(gv: string[]): string[] {
	return gv.filter((v) => /^\d+\.\d+/.test(v))
}

function loaderNames(gv: string[]): string[] {
	return gv
		.filter((v) => /^(Forge|Fabric|Quilt|NeoForge|LiteLoader|Cauldron)/i.test(v))
		.map((v) => v.toLowerCase())
}
</script>

<template>
	<div v-if="loading" class="flex items-center justify-center p-16 text-secondary">
		<SpinnerIcon class="animate-spin h-8 w-8" />
	</div>

	<div v-else-if="loadError" class="flex flex-col items-center gap-4 p-16 text-secondary">
		<p class="m-0">Failed to load project: {{ loadError }}</p>
		<ButtonStyled>
			<button @click="router.back()">Go back</button>
		</ButtonStyled>
	</div>

	<template v-else-if="mod">
		<!-- ── Sidebar ──────────────────────────────────────────────────────── -->
		<Teleport to="#sidebar-teleport-target">
			<!-- 1. Compatibility -->
			<div
				v-if="supportedGameVersions.length || supportedLoaders.length"
				class="project-sidebar-section flex flex-col gap-3"
			>
				<h2 class="text-lg m-0">Compatibility</h2>

				<section v-if="supportedGameVersions.length" class="flex flex-col gap-2">
					<h3 class="text-primary text-base m-0">Minecraft: Java Edition</h3>
					<div class="flex flex-wrap gap-1">
						<TagItem v-for="v in displayGameVersions" :key="v">{{ v }}</TagItem>
					</div>
				</section>

				<section v-if="supportedLoaders.length" class="flex flex-col gap-2">
					<h3 class="text-primary text-base m-0">Platforms</h3>
					<div class="flex flex-wrap gap-1">
						<TagItem
							v-for="loader in supportedLoaders"
							:key="loader"
							:style="`--_color: var(--color-platform-${loader.toLowerCase()})`"
						>
							<component
								:is="getLoaderIcon(loader.toLowerCase())"
								v-if="getLoaderIcon(loader.toLowerCase())"
							/>
							<FormattedTag :tag="loader.toLowerCase()" enforce-type="loader" />
						</TagItem>
					</div>
				</section>

				<section v-if="environmentTags.length" class="flex flex-col gap-2">
					<h3 class="text-primary text-base m-0">Supported environments</h3>
					<div class="flex flex-wrap gap-1">
						<TagItem v-for="tag in environmentTags" :key="tag.label">
							<component :is="tag.icon" aria-hidden="true" />
							{{ tag.label }}
						</TagItem>
					</div>
				</section>
			</div>

			<!-- 2. Links (matches ProjectSidebarLinks' order + heading) -->
			<div
				v-if="mod.links?.websiteUrl || mod.links?.issuesUrl || mod.links?.sourceUrl || mod.links?.wikiUrl"
				class="project-sidebar-section flex flex-col gap-3"
			>
				<h2 class="text-lg m-0">Links</h2>
				<div
					class="flex flex-col gap-3 font-semibold [&>a]:flex [&>a]:gap-2 [&>a]:items-center [&>a]:w-fit [&>a]:text-primary [&>a]:leading-[1.2] [&>a:hover]:underline"
				>
					<a
						v-if="mod.links?.issuesUrl"
						href="#"
						rel="noopener nofollow ugc"
						@click.prevent="openUrl(mod.links!.issuesUrl!)"
					>
						<IssuesIcon aria-hidden="true" />
						Report issues
						<ExternalIcon aria-hidden="true" class="external-icon" />
					</a>
					<a
						v-if="mod.links?.sourceUrl"
						href="#"
						rel="noopener nofollow ugc"
						@click.prevent="openUrl(mod.links!.sourceUrl!)"
					>
						<CodeIcon aria-hidden="true" />
						View source
						<ExternalIcon aria-hidden="true" class="external-icon" />
					</a>
					<a
						v-if="mod.links?.wikiUrl"
						href="#"
						rel="noopener nofollow ugc"
						@click.prevent="openUrl(mod.links!.wikiUrl!)"
					>
						<WikiIcon aria-hidden="true" />
						Visit wiki
						<ExternalIcon aria-hidden="true" class="external-icon" />
					</a>
					<a
						v-if="mod.links?.websiteUrl"
						href="#"
						rel="noopener nofollow ugc"
						@click.prevent="openUrl(mod.links!.websiteUrl!)"
					>
						<ExternalIcon aria-hidden="true" />
						CurseForge page
						<ExternalIcon aria-hidden="true" class="external-icon" />
					</a>
				</div>
			</div>

			<!-- 3. Tags -->
			<div v-if="mod.categories.length" class="project-sidebar-section flex flex-col gap-3">
				<h2 class="text-lg m-0">Tags</h2>
				<div class="flex flex-wrap gap-1">
					<TagItem v-for="cat in mod.categories" :key="cat.id" :action="() => goToCategory(cat)">
						{{ cat.name }}
					</TagItem>
				</div>
			</div>

			<!-- 4. Creators -->
			<div v-if="mod.authors.length" class="project-sidebar-section flex flex-col gap-3">
				<h2 class="text-lg m-0">Creators</h2>
				<div class="flex flex-col gap-3 font-semibold">
					<button
						v-for="author in mod.authors"
						:key="author.id"
						class="flex gap-2 items-center w-fit text-primary leading-[1.2] group bg-transparent border-0 p-0 cursor-pointer text-left"
						@click="openUrl(authorUrl(author))"
					>
						<div class="cf-author-avatar shrink-0">{{ authorInitial(author.name) }}</div>
						<div class="flex flex-col">
							<span class="flex w-full flex-nowrap items-center gap-1 group-hover:underline">
								<span class="min-w-0 overflow-hidden truncate">{{ author.name }}</span>
								<ExternalIcon />
							</span>
							<span class="text-secondary text-sm font-medium">Author</span>
						</div>
					</button>
				</div>
			</div>

			<!-- 5. Details -->
			<div class="project-sidebar-section flex flex-col gap-3">
				<h2 class="text-lg m-0">Details</h2>
				<div
					class="flex flex-col gap-3 font-semibold [&>div]:flex [&>div]:gap-2 [&>div]:items-center"
				>
					<div
						v-tooltip="
							formatCompactNumber(Math.round(mod.downloadCount)) + ' downloads'
						"
					>
						<DownloadIcon aria-hidden="true" />
						<div>
							{{ formatCompactNumber(Math.round(mod.downloadCount)) }} downloads
						</div>
					</div>
					<div v-if="mod.dateCreated" v-tooltip="formatDateTime(mod.dateCreated)">
						<CalendarIcon aria-hidden="true" />
						<div>Published {{ formatRelativeTime(mod.dateCreated) }}</div>
					</div>
					<div v-tooltip="formatDateTime(mod.dateModified)">
						<VersionIcon aria-hidden="true" />
						<div>Updated {{ formatRelativeTime(mod.dateModified) }}</div>
					</div>
				</div>
			</div>
		</Teleport>

		<!-- ── Main content ──────────────────────────────────────────────────── -->
		<div class="flex flex-col gap-4 p-6">
			<InstanceIndicator v-if="instanceFull" :instance="instanceFull" />
			<!-- Header — identical slot structure to ProjectHeader/ContentPageHeader -->
			<ContentPageHeader @contextmenu.prevent.stop="handleRightClick">
				<template #icon>
					<Avatar :src="mod.logo?.thumbnailUrl" :alt="mod.name" size="96px" />
				</template>
				<template #title>{{ mod.name }}</template>
				<template #title-suffix>
					<span class="flex items-center">
						<span class="cf-source-badge">CurseForge</span>
						<span v-tooltip="cfBetaTooltip" class="cf-beta-badge">Beta</span>
					</span>
				</template>
				<template #summary>{{ mod.summary }}</template>
				<template #stats>
					<div
						v-tooltip="
							formatCompactNumber(Math.round(mod.downloadCount)) + ' downloads'
						"
						class="flex items-center gap-2 font-semibold cursor-help"
						:class="{
							'md:border-r md:border-divider md:pr-3':
								!mod.thumbsUpCount && mod.categories.length > 0,
						}"
					>
						<DownloadIcon class="h-6 w-6 text-secondary" />
						{{ formatCompactNumber(Math.round(mod.downloadCount)) }}
					</div>
					<div
						v-if="mod.thumbsUpCount"
						v-tooltip="formatCompactNumber(Math.round(mod.thumbsUpCount)) + ' thumbs up'"
						class="flex items-center gap-2 cursor-help"
						:class="{ 'md:border-r md:border-divider md:pr-3': mod.categories.length > 0 }"
					>
						<HeartIcon class="h-6 w-6 text-secondary" />
						<span class="font-semibold">
							{{ formatCompactNumber(Math.round(mod.thumbsUpCount)) }}
						</span>
					</div>
					<div v-if="mod.categories.length" class="hidden items-center gap-2 md:flex">
						<div class="flex flex-wrap gap-2">
							<TagItem
								v-for="cat in mod.categories.slice(0, 4)"
								:key="cat.id"
								:action="() => goToCategory(cat)"
							>
								{{ cat.name }}
							</TagItem>
						</div>
					</div>
				</template>
				<template #actions>
					<ButtonStyled size="large" color="brand">
						<button
							:disabled="installing || installed || installedInInstance || !canInstall"
							@click="handleInstall"
						>
							<SpinnerIcon v-if="installing" class="animate-spin" />
							<CheckIcon v-else-if="installed || installedInInstance" />
							<DownloadIcon v-else-if="isModpack" />
							<PlusIcon v-else />
							{{
								installing
									? 'Installing…'
									: installed || installedInInstance
										? 'Installed'
										: !canInstall
											? 'Distribution disabled'
											: isModpack
												? 'Install Modpack'
												: instance
													? 'Install'
													: 'Add to Instance'
							}}
						</button>
					</ButtonStyled>
					<ButtonStyled size="large" circular type="transparent">
						<OverflowMenu
							tooltip="More options"
							:options="[
								{
									id: 'open-in-browser',
									link: cfWebsiteUrl,
									external: true,
								},
								{
									divider: true,
								},
								{
									id: 'report',
									color: 'red',
									hoverFilled: true,
									link: 'https://support.curseforge.com/en/support/tickets/new',
									external: true,
								},
							]"
							aria-label="More options"
						>
							<MoreVerticalIcon aria-hidden="true" />
							<template #open-in-browser>
								<ExternalIcon /> Open on CurseForge
							</template>
							<template #report> <ReportIcon /> Report </template>
						</OverflowMenu>
					</ButtonStyled>
				</template>
			</ContentPageHeader>

			<!-- Tabs -->
			<NavTabs
				mode="local"
				:active-index="activeTabIndex"
				:links="tabLinks"
				@tab-click="onTabClick"
			/>

			<!-- Description tab -->
			<Card v-if="activeTab === 'description'">
				<div v-if="description" class="markdown-body" v-html="description" />
				<p v-else class="m-0 text-secondary">{{ mod.summary }}</p>
			</Card>

			<!-- Versions tab -->
			<template v-if="activeTab === 'versions'">
				<div class="flex flex-wrap justify-between gap-2">
					<div class="flex flex-wrap gap-2">
						<DropdownSelect
							v-model="filterGameVersion"
							name="Game version filter"
							class="!w-44"
							:options="[null, ...supportedGameVersions]"
							:display-name="(v: string | null) => v ?? 'All game versions'"
						/>
						<DropdownSelect
							v-if="filterLoaderOptions.length"
							v-model="filterLoader"
							name="Platform filter"
							class="!w-40"
							:options="[null, ...filterLoaderOptions]"
							:display-name="(v: string | null) => v ?? 'All platforms'"
						/>
						<DropdownSelect
							v-model="filterChannel"
							name="Channel filter"
							class="!w-36"
							:options="[null, 'release', 'beta', 'alpha']"
							:display-name="
								(v: string | null) =>
									v ? v.charAt(0).toUpperCase() + v.slice(1) : 'All channels'
							"
						/>
					</div>
					<Pagination
						:page="filesPage"
						class="mt-auto"
						:count="filesPageCount"
						@switch-page="switchFilesPage"
					/>
				</div>
				<div v-if="filesLoading" class="flex items-center justify-center p-8 text-secondary">
					<SpinnerIcon class="animate-spin h-6 w-6" />
				</div>
				<div
					v-else-if="displayedFiles.length"
					class="flex flex-col gap-4 rounded-2xl bg-bg-raised px-6 pb-8 pt-4 sm:px-8 supports-[grid-template-columns:subgrid]:grid supports-[grid-template-columns:subgrid]:grid-cols-[1fr_min-content] supports-[grid-template-columns:subgrid]:sm:grid-cols-[min-content_auto_auto_auto_min-content] supports-[grid-template-columns:subgrid]:xl:grid-cols-[min-content_auto_auto_auto_auto_auto_min-content]"
				>
					<!-- Header row -->
					<div class="cf-vrow">
						<div class="w-9 max-sm:hidden" />
						<div class="text-sm font-bold text-contrast max-sm:hidden">Name</div>
						<div class="text-sm font-bold text-contrast max-sm:hidden sm:max-xl:collapse sm:max-xl:hidden">Game version</div>
						<div class="text-sm font-bold text-contrast max-sm:hidden sm:max-xl:collapse sm:max-xl:hidden">Platforms</div>
						<div class="text-sm font-bold text-contrast max-sm:hidden sm:max-xl:collapse sm:max-xl:hidden">Published</div>
						<div class="text-sm font-bold text-contrast max-sm:hidden sm:max-xl:collapse sm:max-xl:hidden">Downloads</div>
						<div class="w-9 max-sm:hidden" />
					</div>

					<template v-for="file in displayedFiles" :key="file.id">
						<!-- Row divider -->
						<div class="cf-vrow h-px w-full bg-surface-5" />

						<!-- Version row -->
						<div class="cf-vrow group relative">
							<AutoLink
								class="absolute inset-[calc(-1rem-2px)_-2rem] before:absolute before:inset-0 before:transition-all before:content-[''] hover:before:backdrop-brightness-110"
								:to="versionHref(file.id)"
							/>
							<div class="flex flex-col justify-center gap-2 sm:contents">
								<div class="flex flex-row items-center gap-2 sm:contents">
									<!-- Channel indicator -->
									<div class="self-center">
										<div class="relative z-[1]">
											<VersionChannelIndicator :channel="cfChannelType(file.releaseType)" />
										</div>
									</div>
									<!-- Version name -->
									<div class="pointer-events-none relative z-[1] flex flex-col justify-center overflow-hidden min-w-32 group-hover:underline">
										<div class="font-bold text-contrast text-ellipsis overflow-hidden">
											{{ file.displayName }}
										</div>
										<div class="text-xs font-medium text-ellipsis overflow-hidden">
											{{ file.fileName }}
										</div>
									</div>
								</div>
								<div class="flex flex-col justify-center gap-2 sm:contents">
									<div class="flex flex-row flex-wrap items-center gap-1 xl:contents">
										<!-- Game versions -->
										<div class="flex items-center">
											<div class="flex flex-wrap gap-1">
												<TagItem
													v-for="v in mcVersions(file.gameVersions).slice(0, 6)"
													:key="v"
													class="z-[1]"
												>
													{{ v }}
												</TagItem>
												<TagItem
													v-if="mcVersions(file.gameVersions).length > 6"
													class="z-[1]"
												>
													+{{ mcVersions(file.gameVersions).length - 6 }}
												</TagItem>
											</div>
										</div>
										<!-- Loaders -->
										<div class="flex items-center">
											<div class="flex flex-wrap gap-1">
												<TagItem
													v-for="loader in loaderNames(file.gameVersions)"
													:key="loader"
													class="z-[1]"
													:style="`--_color: var(--color-platform-${loader})`"
												>
													<component :is="getLoaderIcon(loader)" v-if="getLoaderIcon(loader)" />
													<FormattedTag :tag="loader" enforce-type="loader" />
												</TagItem>
											</div>
										</div>
									</div>
									<div class="flex flex-col justify-center gap-1 max-sm:flex-row max-sm:justify-start max-sm:gap-3 xl:contents">
										<!-- Published -->
										<div
											v-tooltip="formatDateTime(file.fileDate)"
											class="z-[1] flex cursor-help items-center gap-1 text-nowrap font-medium xl:self-center"
										>
											<CalendarIcon class="xl:hidden" />
											{{ formatRelativeTime(file.fileDate) }}
										</div>
										<!-- Downloads -->
										<div class="pointer-events-none z-[1] flex items-center gap-1 font-medium xl:self-center">
											<DownloadIcon class="xl:hidden" />
											{{ formatCompactNumber(Math.round(file.downloadCount)) }}
										</div>
									</div>
								</div>
							</div>
							<!-- Actions -->
							<div class="flex items-start justify-end gap-1 sm:items-center z-[1] max-[400px]:flex-col max-[400px]:justify-start">
								<ButtonStyled circular type="transparent">
									<button
										v-tooltip="!canInstall ? 'Distribution disabled by author' : instanceFileNames.has(file.fileName) ? 'Installed' : installingFileIds.has(file.id) ? 'Installing…' : isModpack ? 'Install modpack' : 'Install'"
										:class="{ 'group-hover:!bg-brand group-hover:[&>svg]:!text-brand-inverted': !installingFileIds.has(file.id) && canInstall && !instanceFileNames.has(file.fileName) }"
										:disabled="installingFileIds.has(file.id) || !canInstall || instanceFileNames.has(file.fileName)"
										@click.stop="handleVersionInstall(file)"
									>
										<SpinnerIcon v-if="installingFileIds.has(file.id)" class="animate-spin" />
										<CheckIcon v-else-if="instanceFileNames.has(file.fileName)" />
										<DownloadIcon v-else />
									</button>
								</ButtonStyled>
								<ButtonStyled circular type="transparent">
									<a
										v-tooltip="'Open on CurseForge'"
										class="group-hover:!bg-button-bg"
										href="#"
										@click.prevent="openUrl(mod.links?.websiteUrl ? `${mod.links.websiteUrl}/files/${file.id}` : `https://www.curseforge.com/minecraft/mc-mods/${mod!.slug}/files/${file.id}`)"
									>
										<ExternalIcon />
									</a>
								</ButtonStyled>
							</div>
						</div>
					</template>
				</div>
				<Card v-else>
					<p class="m-0 text-secondary text-center py-4">No files found.</p>
				</Card>
				<div v-if="!filesLoading && displayedFiles.length" class="flex">
					<Pagination
						:page="filesPage"
						class="ml-auto"
						:count="filesPageCount"
						@switch-page="switchFilesPage"
					/>
				</div>
			</template>

			<!-- Gallery tab -->
			<template v-if="activeTab === 'gallery'">
				<div class="gallery">
					<Card
						v-for="(img, i) in screenshots"
						:key="i"
						class="gallery-item"
					>
						<a @click="expandImage(img, i)">
							<img
								:src="img.url ?? img.thumbnailUrl"
								:alt="img.title ?? ''"
								class="gallery-image"
							/>
						</a>
						<div v-if="img.title || img.description" class="gallery-body">
							<h3 v-if="img.title">{{ img.title }}</h3>
							<p v-if="img.description" class="m-0 text-secondary text-sm">
								{{ img.description }}
							</p>
						</div>
					</Card>
				</div>
			</template>
		</div>

		<!-- Gallery expand modal (matches Gallery.vue exactly) -->
		<div
			v-if="expandedImage"
			class="expanded-image-modal"
			@click="closeExpand"
		>
			<div class="content">
				<img
					class="image"
					:class="{ 'zoomed-in': zoomedIn }"
					:src="expandedImage.url ?? expandedImage.thumbnailUrl"
					:alt="expandedImage.title ?? 'gallery-image'"
					@click.stop
				/>
				<div class="floating" @click.stop>
					<div class="text">
						<h2 v-if="expandedImage.title">{{ expandedImage.title }}</h2>
						<p v-if="expandedImage.description">{{ expandedImage.description }}</p>
					</div>
					<div class="controls">
						<div class="buttons">
							<ButtonStyled circular>
								<button @click="closeExpand"><XIcon aria-hidden="true" /></button>
							</ButtonStyled>
							<ButtonStyled circular>
								<a
									target="_blank"
									:href="expandedImage.url ?? expandedImage.thumbnailUrl"
								>
									<ExternalIcon aria-hidden="true" />
								</a>
							</ButtonStyled>
							<ButtonStyled circular>
								<button @click="zoomedIn = !zoomedIn">
									<ExpandIcon v-if="!zoomedIn" aria-hidden="true" />
									<ContractIcon v-else aria-hidden="true" />
								</button>
							</ButtonStyled>
							<ButtonStyled v-if="screenshots.length > 1" circular>
								<button @click="prevImage()">
									<LeftArrowIcon aria-hidden="true" />
								</button>
							</ButtonStyled>
							<ButtonStyled v-if="screenshots.length > 1" circular>
								<button @click="nextImage()">
									<RightArrowIcon aria-hidden="true" />
								</button>
							</ButtonStyled>
						</div>
					</div>
				</div>
			</div>
		</div>

		<!-- Instance picker overlay -->
		<CfInstancePicker
			:open="pickerOpen"
			:mod-name="mod.name"
			:mod-icon-url="mod.logo?.thumbnailUrl"
			@close="pickerOpen = false"
			@select="pickerInstall"
		/>

		<ContextMenu ref="options" @option-clicked="handleOptionsClick">
			<template #install> <DownloadIcon /> Install </template>
			<template #open_link> <GlobeIcon /> Open on CurseForge <ExternalIcon /> </template>
			<template #copy_link> <ClipboardCopyIcon /> Copy link </template>
		</ContextMenu>
	</template>
</template>

<style scoped lang="scss">
// Matches project/Index.vue's .project-sidebar-section exactly
.project-sidebar-section {
	@apply p-4 flex flex-col gap-2 border-0 border-b-[1px] border-[--brand-gradient-border] border-solid;
}

// CurseForge source badge
.cf-source-badge {
	display: inline-flex;
	align-items: center;
	padding: 0.15rem 0.5rem;
	background: rgba(241, 100, 54, 0.15);
	color: #f16436;
	border-radius: var(--radius-sm, 0.375rem);
	font-size: 0.75rem;
	font-weight: 600;
	letter-spacing: 0.02em;
	line-height: 1.5;
}

// CF integration status badge — see cfBetaTooltip for the disclaimer text
.cf-beta-badge {
	display: inline-flex;
	align-items: center;
	margin-left: 0.375rem;
	padding: 0.15rem 0.5rem;
	background: var(--color-button-bg);
	color: var(--color-secondary);
	border-radius: var(--radius-sm, 0.375rem);
	font-size: 0.75rem;
	font-weight: 600;
	letter-spacing: 0.02em;
	line-height: 1.5;
	cursor: help;
}

// Author initials avatar (replaces real avatar when no URL available)
// Border + shadow match Avatar.vue's chrome exactly; background/initials are
// our substitution since CF's author data has no avatar image at all.
.cf-author-avatar {
	width: 2rem;
	height: 2rem;
	border: 1px solid var(--surface-5);
	border-radius: 50%;
	background: rgba(142, 132, 207, 0.2);
	box-shadow: var(--shadow-card);
	color: var(--color-secondary);
	font-size: 0.875rem;
	font-weight: 600;
	display: flex;
	align-items: center;
	justify-content: center;
}

// ── Gallery (mirrors Gallery.vue exactly) ──────────────────────────────────────

.gallery {
	display: grid;
	grid-template-columns: repeat(auto-fill, minmax(20rem, 1fr));
	width: 100%;
	gap: 1rem;
}

.gallery-item {
	padding: 0;
	overflow: hidden;
	margin: 0;
	display: flex;
	flex-direction: column;
	cursor: pointer;

	.gallery-image {
		width: 100%;
		aspect-ratio: 2/1;
		object-fit: cover;
		object-position: center;
	}

	.gallery-body {
		flex-grow: 1;
		padding: 1rem;
	}
}

.expanded-image-modal {
	position: fixed;
	z-index: 11;
	overflow: auto;
	top: 0;
	left: 0;
	width: 100%;
	height: 100%;
	background-color: rgba(0, 0, 0, 0.7);
	display: flex;
	justify-content: center;
	align-items: center;

	.content {
		position: relative;
		width: calc(100vw - 2 * var(--gap-lg));
		height: calc(100vh - 2 * var(--gap-lg));

		.image {
			position: absolute;
			left: 50%;
			top: 50%;
			transform: translate(-50%, -50%);
			max-width: calc(100vw - 2 * var(--gap-lg));
			max-height: calc(100vh - 2 * var(--gap-lg));
			border-radius: var(--radius-lg);

			&.zoomed-in {
				object-fit: cover;
				width: auto;
				height: calc(100vh - 2 * var(--gap-lg));
				max-width: calc(100vw - 2 * var(--gap-lg));
			}
		}

		.floating {
			position: absolute;
			left: 50%;
			transform: translateX(-50%);
			bottom: var(--gap-md);
			display: flex;
			flex-direction: column;
			align-items: center;
			gap: var(--gap-md);
			transition: opacity 0.25s ease-in-out;
			opacity: 1;
			padding: 2rem 2rem 0 2rem;

			&:not(&:hover) {
				opacity: 0.4;

				.text {
					transform: translateY(2.5rem) scale(0.8);
					opacity: 0;
				}

				.controls {
					transform: translateY(0.25rem) scale(0.9);
				}
			}

			.text {
				display: flex;
				flex-direction: column;
				max-width: 40rem;
				transition:
					opacity 0.25s ease-in-out,
					transform 0.25s ease-in-out;
				text-shadow: 1px 1px 10px #000000d4;
				margin-bottom: 0.25rem;
				gap: 0.5rem;

				h2 {
					color: var(--dark-color-base);
					font-size: 1.25rem;
					text-align: center;
					margin: 0;
				}

				p {
					color: var(--dark-color-base);
					margin: 0;
				}
			}

			.controls {
				background-color: var(--color-raised-bg);
				padding: var(--gap-md);
				border-radius: var(--radius-md);
				transition:
					opacity 0.25s ease-in-out,
					transform 0.25s ease-in-out;
			}
		}
	}
}

.buttons {
	display: flex;
	gap: 0.5rem;
}

// ── Versions tab ───────────────────────────────────────────────────────────────

.cf-vrow {
	@apply grid grid-cols-[1fr_min-content] gap-4
		supports-[grid-template-columns:subgrid]:col-span-full
		supports-[grid-template-columns:subgrid]:!grid-cols-subgrid
		sm:grid-cols-[min-content_1fr_1fr_1fr_min-content]
		xl:grid-cols-[min-content_1fr_1fr_1fr_auto_auto_min-content];
}

// CurseForge's raw description HTML bakes explicit width/height attributes
// onto every img tag, often inaccurate (e.g. shields.io badges reported far
// larger than their real rendered size). Forcing width back to auto makes
// the browser size every image from its true natural dimensions instead of
// CF's baked-in attribute, matching Modrinth's own descriptions (which carry
// no such attributes and so already render at natural size).
// v-html content isn't scope-attributed, so :deep() is required here — a
// plain nested selector silently fails to match it.
.markdown-body {
	:deep(img) {
		width: auto !important;
	}
}
</style>
