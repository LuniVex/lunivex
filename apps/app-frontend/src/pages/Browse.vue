<script setup lang="ts">
import type { Labrinth } from '@modrinth/api-client'
import {
	CheckIcon,
	ClipboardCopyIcon,
	ExternalIcon,
	GlobeIcon,
	PlusIcon,
	SpinnerIcon,
} from '@modrinth/assets'
import type { BrowseInstallContentType, CardAction, ProjectType, Tags } from '@modrinth/ui'
import {
	BrowsePageLayout,
	BrowseSidebar,
	commonMessages,
	CreationFlowModal,
	defineMessages,
	getLatestMatchingInstallVersion,
	getSelectedInstallPreferences,
	getTargetInstallPreferences,
	injectNotificationManager,
	preferencesDiffer,
	provideBrowseManager,
	requestInstall,
	useBrowseSearch,
	useDebugLogger,
	useVIntl,
} from '@modrinth/ui'
import { useQueryClient } from '@tanstack/vue-query'
import type { Ref } from 'vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type { LocationQuery } from 'vue-router'
import { onBeforeRouteLeave, useRoute, useRouter } from 'vue-router'

import BrowseFeaturedSection from '@/components/ui/BrowseFeaturedSection.vue'
import CfBrowseFeaturedSection, {
	type CfFeaturedMod,
} from '@/components/ui/CfBrowseFeaturedSection.vue'
import ContextMenu from '@/components/ui/ContextMenu.vue'
import { useAppServerBrowse } from '@/composables/browse/use-app-server-browse'
import {
	get_project,
	get_project_v3,
	get_search_results_v3,
	get_version_many,
} from '@/helpers/cache.js'
import {
	curseforge_install_mod,
	curseforge_install_modpack,
	curseforge_search,
	parseCurseForgeSearchParams,
} from '@/helpers/curseforge.js'
import { profile_listener } from '@/helpers/events.js'
import { get_loader_versions as getLoaderManifest } from '@/helpers/metadata'
import {
	get as getInstance,
	get_installed_project_ids as getInstalledProjectIds,
	list as listProfiles,
} from '@/helpers/profile.js'
import { get_categories, get_game_versions, get_loaders } from '@/helpers/tags'
import { resolveIconUrl } from '@/helpers/utils.js'
import { get_profile_worlds } from '@/helpers/worlds'
import { injectContentInstall } from '@/providers/content-install'
import { injectServerInstall } from '@/providers/server-install'
import {
	createServerInstallContent,
	provideServerInstallContent,
} from '@/providers/setup/server-install-content'
import { useBreadcrumbs } from '@/store/breadcrumbs'

const { handleError } = injectNotificationManager()
const { formatMessage } = useVIntl()
const { installingServerProjects, playServerProject, showAddServerToInstanceModal } =
	injectServerInstall()
const { install: installVersion } = injectContentInstall()
const queryClient = useQueryClient()
const debugLog = useDebugLogger('Browse')

const router = useRouter()
const route = useRoute()
const serverSetupModalRef = ref<InstanceType<typeof CreationFlowModal> | null>(null)
const serverInstallContent = createServerInstallContent({ serverSetupModalRef })
provideServerInstallContent(serverInstallContent)
const {
	serverIdQuery,
	serverFlowFrom,
	isFromWorlds,
	isServerContext,
	isSetupServerContext,
	effectiveServerWorldId,
	serverContextServerData,
	serverContentProjectIds,
	queuedServerInstallProjectIds,
	queuedServerInstallCount,
	selectedServerInstallProjects,
	isInstallingQueuedServerInstalls,
	queuedInstallProgress,
	serverBackUrl,
	serverBackLabel,
	serverBrowseHeading,
	clearQueuedServerInstalls,
	removeQueuedServerInstall,
	flushQueuedServerInstalls,
	discardQueuedServerInstallsAndBack,
	installQueuedServerInstallsAndBack,
	initServerContext,
	watchServerContextChanges,
	searchServerModpacks,
	getServerProjectVersions,
	enforceSetupModpackRoute,
	getQueuedServerInstallPlans,
	setQueuedServerInstallPlans,
	openServerModpackInstallFlow,
	onServerFlowBack,
	handleServerModpackFlowCreate,
	markServerProjectInstalled,
} = serverInstallContent

debugLog('fetching tags (categories, loaders, gameVersions)')
const [categories, loaders, availableGameVersions] = await Promise.all([
	get_categories()
		.catch(handleError)
		.then(ref<Labrinth.Tags.v2.Category[]>),
	get_loaders()
		.catch(handleError)
		.then(ref<Labrinth.Tags.v2.Loader[]>),
	get_game_versions()
		.catch(handleError)
		.then(ref<Labrinth.Tags.v2.GameVersion[]>),
])

const tags: Ref<Tags> = computed(() => ({
	gameVersions: availableGameVersions.value ?? [],
	loaders: loaders.value ?? [],
	categories: categories.value ?? [],
}))

type Instance = {
	game_version: string
	loader: string
	path: string
	install_stage: string
	icon_path?: string
	name: string
	linked_data?: {
		project_id: string
		version_id: string
		locked: boolean
	}
}

const instance: Ref<Instance | null> = ref(null)
const installedProjectIds: Ref<string[] | null> = ref(null)
const instanceHideInstalled = ref(false)
const newlyInstalled = ref<string[]>([])
const hiddenInstanceProjectIds = ref<Set<string>>(new Set())
const hiddenInstanceProjectIdsInitialized = ref(false)
const isServerInstance = ref(false)

if (isFromWorlds.value && route.params.projectType !== 'server') {
	router.replace({
		path: '/browse/server',
		query: route.query,
	})
}

enforceSetupModpackRoute(route.params.projectType as string | undefined)

const allInstalledIds = computed(
	() => new Set([...newlyInstalled.value, ...(installedProjectIds.value ?? [])]),
)

function syncHiddenInstanceProjectIds() {
	hiddenInstanceProjectIds.value = new Set([
		...(installedProjectIds.value ?? []),
		...newlyInstalled.value,
	])
	hiddenInstanceProjectIdsInitialized.value = true
}

watch(
	installedProjectIds,
	(ids) => {
		if (!ids) return
		if (!hiddenInstanceProjectIdsInitialized.value) {
			syncHiddenInstanceProjectIds()
		}
	},
	{ immediate: true },
)

watchServerContextChanges()

await initInstanceContext()

async function refreshInstalledProjectIds() {
	if (!route.query.i) return

	if (route.query.from === 'worlds') {
		const worlds = await get_profile_worlds(route.query.i as string).catch(handleError)
		if (!worlds) return

		const serverProjectIds = worlds
			.filter((w) => w.type === 'server' && 'project_id' in w && w.project_id)
			.map((w) => (w as { project_id: string }).project_id)
		debugLog('installedServerProjectIds loaded', { count: serverProjectIds.length })
		installedProjectIds.value = serverProjectIds
		return
	}

	const ids = await getInstalledProjectIds(route.query.i as string).catch(handleError)
	if (!ids) return

	debugLog('installedProjectIds loaded', { count: ids.length })
	installedProjectIds.value = ids
}

async function initInstanceContext() {
	debugLog('initInstanceContext', {
		queryI: route.query.i,
		queryAi: route.query.ai,
		querySid: route.query.sid,
		queryWid: route.query.wid,
		queryFrom: route.query.from,
	})
	await initServerContext()

	if (route.query.i) {
		instance.value = (await getInstance(route.query.i as string).catch(handleError)) ?? null
		debugLog('instance loaded', {
			name: instance.value?.name,
			loader: instance.value?.loader,
			gameVersion: instance.value?.game_version,
		})

		await refreshInstalledProjectIds()

		if (instance.value?.linked_data?.project_id) {
			debugLog('checking linked project for server status', instance.value.linked_data.project_id)
			const projectV3 = await get_project_v3(
				instance.value.linked_data.project_id,
				'must_revalidate',
			).catch(handleError)
			if (projectV3?.minecraft_server != null) {
				debugLog('instance is a server instance')
				isServerInstance.value = true
			}
		}
	}

	if (route.query.ai && !(route.params.projectType === 'modpack')) {
		debugLog('setting instanceHideInstalled from query', route.query.ai)
		instanceHideInstalled.value = route.query.ai === 'true'
	}
}

const instanceFilters = computed(() => {
	const filters = []

	if (instance.value) {
		const gameVersion = instance.value.game_version
		if (gameVersion) {
			filters.push({ type: 'game_version', option: gameVersion })
		}

		const platform = instance.value.loader
		const supportedModLoaders = ['fabric', 'forge', 'quilt', 'neoforge']

		if (platform && projectType.value === 'mod' && supportedModLoaders.includes(platform)) {
			filters.push({ type: 'mod_loader', option: platform })
		}

		if (isServerInstance.value) {
			filters.push({ type: 'environment', option: 'client' })
		}

		if (instanceHideInstalled.value && hiddenInstanceProjectIds.value.size > 0) {
			for (const id of hiddenInstanceProjectIds.value) {
				filters.push({ type: 'project_id', option: `project_id:${id}`, negative: true })
			}
		}
	}

	return filters
})

const serverHideInstalled = ref(false)
const hideSelectedServerInstalls = ref(false)
if (route.query.shi) {
	serverHideInstalled.value = route.query.shi === 'true'
}
const hiddenServerContentProjectIds = ref<Set<string>>(new Set())
const hiddenServerContentProjectIdsInitialized = ref(false)

function syncHiddenServerContentProjectIds() {
	hiddenServerContentProjectIds.value = new Set(serverContentProjectIds.value)
	hiddenServerContentProjectIdsInitialized.value = true
}

watch(
	serverContentProjectIds,
	() => {
		if (!hiddenServerContentProjectIdsInitialized.value) {
			syncHiddenServerContentProjectIds()
		}
	},
	{ immediate: true },
)

const serverContextFilters = computed(() => {
	const filters: { type: string; option: string; negative?: boolean }[] = []
	if (!serverContextServerData.value) return filters
	const pt = projectType.value

	if (pt !== 'modpack') {
		const gameVersion = serverContextServerData.value.mc_version
		if (gameVersion) filters.push({ type: 'game_version', option: gameVersion })

		const platform = serverContextServerData.value.loader?.toLowerCase()
		if (platform && ['fabric', 'forge', 'quilt', 'neoforge'].includes(platform))
			filters.push({ type: 'mod_loader', option: platform })
		if (platform && ['paper', 'purpur'].includes(platform))
			filters.push({ type: 'plugin_loader', option: platform })

		if (pt === 'mod') filters.push({ type: 'environment', option: 'server' })

		if (hideSelectedServerInstalls.value && queuedServerInstallProjectIds.value.size > 0) {
			for (const id of queuedServerInstallProjectIds.value) {
				filters.push({ type: 'project_id', option: `project_id:${id}`, negative: true })
			}
		}
	}

	if (pt === 'modpack') {
		filters.push(
			{ type: 'environment', option: 'client' },
			{ type: 'environment', option: 'server' },
		)
	}

	if (serverHideInstalled.value && hiddenServerContentProjectIds.value.size > 0) {
		for (const id of hiddenServerContentProjectIds.value) {
			filters.push({ type: 'project_id', option: `project_id:${id}`, negative: true })
		}
	}

	return filters
})

const combinedProvidedFilters = computed(() =>
	isServerContext.value ? serverContextFilters.value : instanceFilters.value,
)

const {
	serverPings,
	contextMenuRef,
	updateServerHits,
	getServerModpackContent,
	getServerCardActions,
	handleRightClick,
	handleOptionsClick,
} = useAppServerBrowse({
	instance,
	isFromWorlds,
	allInstalledIds,
	newlyInstalled,
	installingServerProjects,
	playServerProject,
	showAddServerToInstanceModal,
	handleError: (err: unknown) => handleError(err as Error),
	router,
})

const offline = ref(!navigator.onLine)
window.addEventListener('offline', () => {
	debugLog('went offline')
	offline.value = true
})
window.addEventListener('online', () => {
	debugLog('went online')
	offline.value = false
})

const messages = defineMessages({
	addServersToInstance: {
		id: 'app.browse.add-servers-to-instance',
		defaultMessage: 'Adding server to instance',
	},
	addToAnInstance: {
		id: 'app.browse.add-to-an-instance',
		defaultMessage: 'Add to an instance',
	},
	discoverContent: {
		id: 'app.browse.discover-content',
		defaultMessage: 'Discover content',
	},
	discoverServers: {
		id: 'app.browse.discover-servers',
		defaultMessage: 'Discover servers',
	},
	environmentProvidedByServer: {
		id: 'search.filter.locked.server-environment.title',
		defaultMessage: 'Only client-side mods can be added to the server instance',
	},
	gameVersionProvidedByInstance: {
		id: 'search.filter.locked.instance-game-version.title',
		defaultMessage: 'Game version is provided by the instance',
	},
	gameVersionProvidedByServer: {
		id: 'search.filter.locked.server-game-version.title',
		defaultMessage: 'Game version is provided by the server',
	},
	hideAddedServers: {
		id: 'app.browse.hide-added-servers',
		defaultMessage: 'Hide already added servers',
	},
	installingToServer: {
		id: 'app.browse.server.installing',
		defaultMessage: 'Installing',
	},
	backToInstance: {
		id: 'app.browse.back-to-instance',
		defaultMessage: 'Back to instance',
	},
	serverInstanceContentWarning: {
		id: 'app.browse.server-instance-content-warning',
		defaultMessage:
			'Adding content can break compatibility when joining the server. Any added content will also be lost when you update the server instance content.',
	},
	modLoaderProvidedByInstance: {
		id: 'search.filter.locked.instance-loader.title',
		defaultMessage: 'Loader is provided by the instance',
	},
	modpacksProjectType: {
		id: 'app.browse.project-type.modpacks',
		defaultMessage: 'Modpacks',
	},
	modLoaderProvidedByServer: {
		id: 'search.filter.locked.server-loader.title',
		defaultMessage: 'Loader is provided by the server',
	},
	providedByInstance: {
		id: 'search.filter.locked.instance',
		defaultMessage: 'Provided by the instance',
	},
	providedByServer: {
		id: 'search.filter.locked.server',
		defaultMessage: 'Provided by the server',
	},
	syncFilterButton: {
		id: 'search.filter.locked.instance.sync',
		defaultMessage: 'Sync with instance',
	},
})

const breadcrumbs = useBreadcrumbs()
const browseTitle = computed(() =>
	formatMessage(isFromWorlds.value ? messages.discoverServers : messages.discoverContent),
)
breadcrumbs.setName('BrowseTitle', browseTitle.value)
if (instance.value) {
	const instanceLink = `/instance/${encodeURIComponent(instance.value.path)}`
	breadcrumbs.setContext({
		name: instance.value.name,
		link: isFromWorlds.value ? `${instanceLink}/worlds` : instanceLink,
	})
} else {
	breadcrumbs.setContext(null)
}

onBeforeRouteLeave(() => {
	breadcrumbs.setContext({
		name: browseTitle.value,
		link: `/browse/${projectType.value}`,
		query: route.query,
	})
})

const projectType = ref<ProjectType>(route.params.projectType as ProjectType)

const cfBetaTooltip =
	"CurseForge integration is new - some pages may not match Modrinth's layout exactly yet, and bugs are possible. Report issues on Discord."

const CF_SUPPORTED_TYPES = new Set(['mod', 'modpack', 'resourcepack', 'shader'])
const CF_CLASS_IDS: Record<string, number> = {
	mod: 6,
	modpack: 4471,
	resourcepack: 12,
	shader: 6552,
}

type BrowseSource = 'modrinth' | 'curseforge'
const source = ref<BrowseSource>(
	route.query.src === 'curseforge' && CF_SUPPORTED_TYPES.has(projectType.value)
		? 'curseforge'
		: 'modrinth',
)

const showCurseForgeToggle = computed(
	() =>
		CF_SUPPORTED_TYPES.has(projectType.value) &&
		!isServerContext.value &&
		!isFromWorlds.value &&
		!instance.value,
)

watch(
	() => route.params.projectType as ProjectType,
	async (newType) => {
		if (isSetupServerContext.value) {
			enforceSetupModpackRoute(newType)
			if (newType !== 'modpack') return
		}

		if (!newType || newType === projectType.value) return

		debugLog('projectType route param changed', { from: projectType.value, to: newType })
		projectType.value = newType

		if (!CF_SUPPORTED_TYPES.has(newType)) {
			source.value = 'modrinth'
		}

		if (!route.query.i && instance.value) {
			debugLog('instance context removed, resetting')
			instance.value = null
			installedProjectIds.value = null
			instanceHideInstalled.value = false
			newlyInstalled.value = []
			isServerInstance.value = false
			breadcrumbs.setName('BrowseTitle', formatMessage(messages.discoverContent))
			breadcrumbs.setContext(null)
		}
	},
)

const selectableProjectTypes = computed(() => {
	let dataPacks = false,
		mods = false,
		modpacks = false

	if (instance.value) {
		if (
			availableGameVersions.value &&
			availableGameVersions.value.findIndex((x) => x.version === instance.value?.game_version) <=
				availableGameVersions.value.findIndex((x) => x.version === '1.13') &&
			!isServerInstance.value
		) {
			dataPacks = true
		}

		if (instance.value.loader !== 'vanilla') {
			mods = true
		}
	} else {
		dataPacks = true
		mods = true
		modpacks = true
	}

	const params: LocationQuery = {}

	if (route.query.i) params.i = route.query.i
	if (route.query.ai) params.ai = route.query.ai
	if (route.query.from) params.from = route.query.from
	if (route.query.sid) params.sid = route.query.sid
	if (effectiveServerWorldId.value) params.wid = effectiveServerWorldId.value

	const queryString = new URLSearchParams(params as Record<string, string>).toString()
	const suffix = queryString ? `?${queryString}` : ''

	if (isSetupServerContext.value) {
		return [
			{ label: formatMessage(messages.modpacksProjectType), href: `/browse/modpack${suffix}` },
		]
	}

	if (isFromWorlds.value) {
		return [{ label: 'Servers', href: `/browse/server${suffix}` }]
	}

	return [
		{ label: 'Modpacks', href: `/browse/modpack${suffix}`, shown: modpacks },
		{ label: 'Mods', href: `/browse/mod${suffix}`, shown: mods },
		{ label: 'Resource Packs', href: `/browse/resourcepack${suffix}` },
		{ label: 'Data Packs', href: `/browse/datapack${suffix}`, shown: dataPacks },
		{ label: 'Shaders', href: `/browse/shader${suffix}` },
		{ label: 'Servers', href: `/browse/server${suffix}`, shown: !instance.value },
	]
})

const installContext = computed(() => {
	if (isServerContext.value && serverContextServerData.value) {
		return {
			name: serverContextServerData.value.name,
			loader: serverContextServerData.value.loader ?? '',
			gameVersion: serverContextServerData.value.mc_version ?? '',
			serverId: serverIdQuery.value,
			upstream: serverContextServerData.value.upstream,
			iconSrc: null as string | null,
			isMedal: serverContextServerData.value.is_medal,
			backUrl: serverBackUrl.value,
			backLabel: serverBackLabel.value,
			heading: serverBrowseHeading.value,
			queuedCount: queuedServerInstallCount.value,
			selectedProjects: selectedServerInstallProjects.value,
			isInstallingSelected: isInstallingQueuedServerInstalls.value,
			installProgress: queuedInstallProgress.value,
			clearQueued: clearQueuedServerInstalls,
			clearSelected: clearQueuedServerInstalls,
			onBack: flushQueuedServerInstalls,
			discardSelectedAndBack: discardQueuedServerInstallsAndBack,
			installSelected: installQueuedServerInstallsAndBack,
		}
	}
	if (instance.value) {
		return {
			name: instance.value.name,
			loader: instance.value.loader,
			gameVersion: instance.value.game_version,
			iconSrc: resolveIconUrl(instance.value.icon_path),
			backUrl: `/instance/${encodeURIComponent(instance.value.path)}${isFromWorlds.value ? '/worlds' : ''}`,
			backLabel: formatMessage(messages.backToInstance),
			heading: formatMessage(
				isFromWorlds.value ? messages.addServersToInstance : commonMessages.installingContentLabel,
			),
			warning:
				isServerInstance.value && !isFromWorlds.value
					? formatMessage(messages.serverInstanceContentWarning)
					: undefined,
		}
	}
	return null
})

const installingProjectIds = ref<Set<string>>(new Set())
const installedCfProjectIds = ref<Set<string>>(new Set())

type CfPickerMod = { modId: number; projectType: string; title: string; iconUrl?: string }
const cfPicker = ref<CfPickerMod | null>(null)
const cfPickerInstances = ref<
	{ path: string; name: string; loader: string; gameVersion: string; iconPath?: string }[]
>([])

async function openCfInstancePicker(
	modId: number,
	projectTypeVal: string,
	title: string,
	iconUrl?: string,
) {
	try {
		const all = await listProfiles()
		cfPickerInstances.value = all
			.filter((p) => p.install_stage === 'installed')
			.map((p) => ({
				path: p.path,
				name: p.name,
				loader: p.loader,
				gameVersion: p.game_version,
				iconPath: p.icon_path,
			}))
	} catch {
		cfPickerInstances.value = []
	}
	cfPicker.value = { modId, projectType: projectTypeVal, title, iconUrl }
}

async function cfPickerInstall(
	instancePath: string,
	instanceLoader: string,
	instanceGameVersion: string,
) {
	const pending = cfPicker.value
	if (!pending) return
	cfPicker.value = null
	const cfId = `cf_${pending.modId}`
	setProjectInstalling(cfId, true)
	try {
		await curseforge_install_mod({
			profilePath: instancePath,
			modId: pending.modId,
			gameVersion: instanceGameVersion,
			loader: pending.projectType === 'mod' ? instanceLoader : null,
			projectType: pending.projectType,
		})
		installedCfProjectIds.value = new Set([...installedCfProjectIds.value, cfId])
	} catch (err) {
		handleError(err as Error)
	} finally {
		setProjectInstalling(cfId, false)
	}
}

function setProjectInstalling(projectId: string, installing: boolean) {
	const next = new Set(installingProjectIds.value)
	if (installing) {
		next.add(projectId)
	} else {
		next.delete(projectId)
	}
	installingProjectIds.value = next
}

const serverInstallQueue = {
	get: getQueuedServerInstallPlans,
	set: setQueuedServerInstallPlans,
}

function getCurrentSelectedInstallPreferences(projectTypeValue: string) {
	return getSelectedInstallPreferences({
		contentType: projectTypeValue,
		selectedFilters: searchState.currentFilters.value,
		providedFilters: combinedProvidedFilters.value,
		overriddenProvidedFilterTypes: searchState.overriddenProvidedFilterTypes.value,
	})
}

function getServerInstallTargetPreferences(contentType: BrowseInstallContentType) {
	return getTargetInstallPreferences(
		{
			gameVersion: serverContextServerData.value?.mc_version,
			loader: serverContextServerData.value?.loader,
		},
		contentType,
	)
}

function getInstanceInstallTargetPreferences(projectTypeValue: string) {
	return getTargetInstallPreferences(
		{
			gameVersion: instance.value?.game_version,
			loader: instance.value?.loader,
		},
		projectTypeValue,
	)
}

async function getInstallProjectVersions(projectId: string) {
	const project = await get_project(projectId, 'must_revalidate')
	return (await get_version_many(
		project.versions,
		'must_revalidate',
	)) as Labrinth.Versions.v2.Version[]
}

async function chooseInstanceInstallVersion(
	project: Labrinth.Search.v2.ResultSearchProject & Labrinth.Search.v3.ResultSearchProject,
	projectTypeValue: string,
) {
	const targetInstance = instance.value
	if (!targetInstance) {
		return { versionId: null as string | null }
	}

	const selectedPreferences = getCurrentSelectedInstallPreferences(projectTypeValue)
	const targetPreferences = getInstanceInstallTargetPreferences(projectTypeValue)
	if (!preferencesDiffer(selectedPreferences, targetPreferences)) {
		return { versionId: null as string | null }
	}

	const selectedVersion = getLatestMatchingInstallVersion(
		await getInstallProjectVersions(project.project_id),
		selectedPreferences,
		projectTypeValue,
	)

	if (!selectedVersion) {
		return { versionId: null as string | null }
	}

	return { versionId: selectedVersion.id }
}

function getCardActions(
	result: Labrinth.Search.v2.ResultSearchProject | Labrinth.Search.v3.ResultSearchProject,
	currentProjectType: string,
): CardAction[] {
	if (currentProjectType === 'server') {
		return getServerCardActions(result as Labrinth.Search.v3.ResultSearchProject)
	}

	// CurseForge result — separate install path
	const anyResult = result as Labrinth.Search.v2.ResultSearchProject & {
		_source?: string
		_cfModId?: number
		_allowDist?: boolean
		installed?: boolean
		installing?: boolean
	}
	if (anyResult._source === 'curseforge') {
		const cfId = anyResult.project_id
		const cfModId = anyResult._cfModId!
		const isInstalling = installingProjectIds.value.has(cfId)
		const isInstalled = installedCfProjectIds.value.has(cfId)
		const canInstall = anyResult._allowDist !== false

		if (!instance.value) {
			// Modpacks: install directly as a new instance (classId 4471)
			if (currentProjectType === 'modpack') {
				return [
					{
						key: 'install',
						label: isInstalling ? 'Installing…' : isInstalled ? 'Installed' : 'Install Modpack',
						icon: isInstalling ? SpinnerIcon : isInstalled ? CheckIcon : PlusIcon,
						iconClass: isInstalling ? 'animate-spin' : undefined,
						disabled: isInstalling || isInstalled || !canInstall,
						tooltip: !canInstall ? 'Author has disabled distribution' : undefined,
						color: 'brand' as const,
						type: 'standard' as const,
						onClick: async () => {
							if (!canInstall) return
							setProjectInstalling(cfId, true)
							try {
								await curseforge_install_modpack(cfModId)
								installedCfProjectIds.value = new Set([...installedCfProjectIds.value, cfId])
							} catch (err) {
								handleError(err as Error)
							} finally {
								setProjectInstalling(cfId, false)
							}
						},
					},
				]
			}

			// Mods/other: show instance picker
			return [
				{
					key: 'install',
					label: isInstalling ? formatMessage(messages.installingToServer) : 'Add to Instance',
					icon: isInstalling ? SpinnerIcon : PlusIcon,
					iconClass: isInstalling ? 'animate-spin' : undefined,
					disabled: isInstalling || isInstalled || !canInstall,
					tooltip: !canInstall ? 'Author has disabled distribution' : undefined,
					color: 'brand' as const,
					type: 'standard' as const,
					onClick: async () => {
						if (!canInstall) return
						await openCfInstancePicker(
							cfModId,
							currentProjectType,
							anyResult.title,
							anyResult.icon_url ?? undefined,
						)
					},
				},
			]
		}

		return [
			{
				key: 'install',
				label: formatMessage(
					isInstalling
						? messages.installingToServer
						: isInstalled
							? commonMessages.installedLabel
							: commonMessages.installButton,
				),
				icon: isInstalling ? SpinnerIcon : isInstalled ? CheckIcon : PlusIcon,
				iconClass: isInstalling ? 'animate-spin' : undefined,
				disabled: isInstalled || isInstalling || !canInstall,
				tooltip: !canInstall ? 'Author has disabled distribution' : undefined,
				color: 'brand' as const,
				type: 'standard' as const,
				onClick: async () => {
					if (!instance.value) return
					setProjectInstalling(cfId, true)
					try {
						await curseforge_install_mod({
							profilePath: instance.value.path,
							modId: cfModId,
							gameVersion: instance.value.game_version,
							loader: currentProjectType === 'mod' ? instance.value.loader : null,
							projectType: currentProjectType,
						})
						installedCfProjectIds.value = new Set([...installedCfProjectIds.value, cfId])
					} catch (err) {
						handleError(err as Error)
					} finally {
						setProjectInstalling(cfId, false)
					}
				},
			},
		]
	}

	// Non-server Modrinth project actions
	const projectResult = anyResult as (Labrinth.Search.v2.ResultSearchProject &
		Labrinth.Search.v3.ResultSearchProject) & {
		installed?: boolean
		installing?: boolean
	}
	const isInstalled =
		projectResult.installed ||
		allInstalledIds.value.has(projectResult.project_id || '') ||
		serverContentProjectIds.value.has(projectResult.project_id || '') ||
		serverContextServerData.value?.upstream?.project_id === projectResult.project_id
	const isInstalling = installingProjectIds.value.has(projectResult.project_id)

	if (
		isServerContext.value &&
		['modpack', 'mod', 'plugin', 'datapack'].includes(currentProjectType)
	) {
		const isQueued = queuedServerInstallProjectIds.value.has(projectResult.project_id)
		const isInstallingSelection = isInstallingQueuedServerInstalls.value
		const validatingInstall =
			isInstalling && currentProjectType !== 'modpack' && !isInstallingSelection
		const installLabel = isInstalled
			? commonMessages.installedLabel
			: isQueued
				? isInstalling || isInstallingSelection
					? validatingInstall
						? commonMessages.validatingLabel
						: messages.installingToServer
					: commonMessages.selectedLabel
				: isInstalling || isInstallingSelection
					? validatingInstall
						? commonMessages.validatingLabel
						: messages.installingToServer
					: commonMessages.installButton
		return [
			{
				key: 'install',
				label: formatMessage(installLabel),
				icon:
					isInstalling || isInstallingSelection
						? SpinnerIcon
						: isQueued || isInstalled
							? CheckIcon
							: PlusIcon,
				iconClass: isInstalling || isInstallingSelection ? 'animate-spin' : undefined,
				disabled: isInstalled || isInstalling || isInstallingSelection,
				color: isQueued && !isInstalling && !isInstallingSelection ? 'green' : 'brand',
				type: 'standard',
				onClick: async () => {
					if (isQueued) {
						removeQueuedServerInstall(projectResult.project_id)
						return
					}

					const contentType = currentProjectType as BrowseInstallContentType
					const isModpack = contentType === 'modpack'
					const shouldShowInstalling = isModpack || !isQueued
					if (shouldShowInstalling) {
						setProjectInstalling(projectResult.project_id, true)
					}
					try {
						await requestInstall({
							project: projectResult,
							contentType,
							mode: isModpack ? 'immediate' : 'queue',
							selectedFilters: isModpack ? [] : searchState.currentFilters.value,
							providedFilters: isModpack ? [] : combinedProvidedFilters.value,
							overriddenProvidedFilterTypes: isModpack
								? []
								: searchState.overriddenProvidedFilterTypes.value,
							targetPreferences: getServerInstallTargetPreferences(contentType),
							getProjectVersions: getInstallProjectVersions,
							queue: serverInstallQueue,
							install: (plan) =>
								openServerModpackInstallFlow({
									projectId: plan.projectId,
									versionId: plan.versionId,
									name: plan.project.name,
									iconUrl: plan.project.icon_url ?? undefined,
								}),
						})
					} catch (err) {
						handleError(err as Error)
					} finally {
						if (shouldShowInstalling) {
							setProjectInstalling(projectResult.project_id, false)
						}
					}
				},
			},
		]
	}

	const isModpack = projectResult.project_types?.includes('modpack')
	const shouldUseInstallIcon = !!instance.value || isModpack

	return [
		{
			key: 'install',
			label: formatMessage(
				isInstalling
					? messages.installingToServer
					: isInstalled
						? commonMessages.installedLabel
						: shouldUseInstallIcon
							? commonMessages.installButton
							: messages.addToAnInstance,
			),
			icon: isInstalling ? SpinnerIcon : isInstalled ? CheckIcon : PlusIcon,
			iconClass: isInstalling ? 'animate-spin' : undefined,
			disabled: isInstalled || isInstalling,
			color: 'brand',
			type: 'standard',
			onClick: async () => {
				setProjectInstalling(projectResult.project_id, true)
				try {
					const selectedInstall = instance.value
						? await chooseInstanceInstallVersion(projectResult, currentProjectType)
						: { versionId: null as string | null }
					if (selectedInstall === null) {
						setProjectInstalling(projectResult.project_id, false)
						return
					}
					const selectedPreferences = getCurrentSelectedInstallPreferences(currentProjectType)
					await installVersion(
						projectResult.project_id,
						selectedInstall.versionId,
						instance.value ? instance.value.path : null,
						'SearchCard',
						(versionId, installedProjectIds) => {
							setProjectInstalling(projectResult.project_id, false)
							if (versionId) {
								onSearchResultsInstalled(installedProjectIds ?? [projectResult.project_id])
							}
						},
						(profile) => {
							router.push(`/instance/${profile}`)
						},
						{
							preferredLoader: instance.value?.loader ?? selectedPreferences.loaders?.[0],
							preferredGameVersion:
								instance.value?.game_version ?? selectedPreferences.gameVersions?.[0],
						},
					)
				} catch (err) {
					setProjectInstalling(projectResult.project_id, false)
					handleError(err as Error)
				}
			},
		},
	]
}

function onSearchResultInstalled(id: string) {
	if (isServerContext.value) {
		markServerProjectInstalled(id)
		return
	}
	if (!newlyInstalled.value.includes(id)) {
		newlyInstalled.value = [...newlyInstalled.value, id]
	}
}

function onSearchResultsInstalled(ids: string[]) {
	if (isServerContext.value) {
		for (const id of ids) {
			markServerProjectInstalled(id)
		}
		return
	}
	newlyInstalled.value = Array.from(new Set([...newlyInstalled.value, ...ids]))
}

async function search(requestParams: string) {
	debugLog('searching', requestParams)
	const isServer = projectType.value === 'server'
	const isCfMode =
		source.value === 'curseforge' &&
		CF_SUPPORTED_TYPES.has(projectType.value) &&
		!isServerContext.value &&
		!isFromWorlds.value

	if (isCfMode) {
		const searchParams = parseCurseForgeSearchParams(requestParams)
		const limit = searchParams.pageSize
		let cfResults: {
			data: {
				id: number
				name: string
				slug: string
				summary: string
				downloadCount: number
				categories: { id: number; name: string }[]
				authors: { id: number; name: string }[]
				logo?: { thumbnailUrl?: string }
				links?: { websiteUrl?: string }
				allowModDistribution?: boolean
			}[]
			pagination: { index: number; pageSize: number; resultCount: number; totalCount: number }
		} | null = null

		try {
			const preferences = getCurrentSelectedInstallPreferences(projectType.value)
			cfResults = await curseforge_search({
				...searchParams,
				gameVersions: preferences.gameVersions ?? null,
				loaders: projectType.value === 'mod' ? (preferences.loaders ?? null) : null,
				categoryId: route.query.cfcat ? Number(route.query.cfcat) : null,
				classId: CF_CLASS_IDS[projectType.value] ?? 6,
			})
		} catch (e: unknown) {
			debugLog('CF search error', e)
			handleError(e as Error)
			throw e
		}

		if (!cfResults) {
			return { projectHits: [], serverHits: [], total_hits: 0, per_page: limit }
		}

		const cfHits = cfResults.data.map((cf) => {
			const cats = cf.categories.map((c) => c.name.toLowerCase().replace(/[\s&+/]+/g, '-'))
			return {
				project_id: `cf_${cf.id}`,
				project_types: [projectType.value],
				slug: cf.slug,
				author: cf.authors[0]?.name ?? '',
				organization: null,
				author_id: null,
				organization_id: null,
				title: cf.name,
				description: cf.summary,
				categories: cats,
				display_categories: cats,
				downloads: cf.downloadCount,
				follows: 0,
				icon_url: cf.logo?.thumbnailUrl ?? undefined,
				date_created: '',
				date_modified: '',
				latest_version: '',
				license: 'arr',
				client_side: 'optional',
				server_side: 'optional',
				featured_gallery: null,
				color: undefined,
				_source: 'curseforge',
				_cfModId: cf.id,
				_cfWebsiteUrl: cf.links?.websiteUrl ?? undefined,
				_allowDist: cf.allowModDistribution !== false,
			} as unknown as Labrinth.Search.v2.ResultSearchProject & { _source: string }
		})

		return {
			projectHits: cfHits,
			serverHits: [],
			total_hits: cfResults.pagination.totalCount,
			per_page: cfResults.pagination.pageSize,
		}
	}

	const rawResults = await queryClient.fetchQuery({
		queryKey: ['search', 'v3', requestParams],
		queryFn: () =>
			get_search_results_v3(requestParams) as Promise<{
				result: Labrinth.Search.v3.SearchResults & {
					hits: (Labrinth.Search.v3.ResultSearchProject & { installed?: boolean })[]
				}
			} | null>,
		staleTime: 30_000,
	})

	if (!rawResults) {
		return { projectHits: [], serverHits: [], total_hits: 0, per_page: 20 }
	}

	if (isServer) {
		const hits = rawResults.result.hits ?? []
		updateServerHits(hits)
		return {
			projectHits: [],
			serverHits: hits,
			total_hits: rawResults.result.total_hits ?? 0,
			per_page: rawResults.result.hits_per_page,
		}
	}

	const mrHits = rawResults.result.hits.map((hit) => {
		const mapped = {
			...hit,
			title: hit.name,
			description: hit.summary,
		} as unknown as Labrinth.Search.v2.ResultSearchProject & { installed?: boolean }

		if (instance.value || isServerContext.value) {
			const installedIds = instance.value
				? new Set([...newlyInstalled.value, ...(installedProjectIds.value ?? [])])
				: serverContentProjectIds.value
			mapped.installed = installedIds.has(hit.project_id)
		}

		return mapped
	})

	return {
		projectHits: mrHits,
		serverHits: [],
		total_hits: rawResults.result.total_hits,
		per_page: rawResults.result.hits_per_page,
	}
}

const isServerFilterContext = computed(() => isServerContext.value || isServerInstance.value)

const lockedFilterMessages = computed(() => ({
	gameVersion: formatMessage(
		isServerFilterContext.value
			? messages.gameVersionProvidedByServer
			: messages.gameVersionProvidedByInstance,
	),
	modLoader: formatMessage(
		isServerFilterContext.value
			? messages.modLoaderProvidedByServer
			: messages.modLoaderProvidedByInstance,
	),
	environment: formatMessage(messages.environmentProvidedByServer),
	syncButton: formatMessage(messages.syncFilterButton),
	providedBy: formatMessage(
		isServerFilterContext.value ? messages.providedByServer : messages.providedByInstance,
	),
}))

const searchState = useBrowseSearch({
	projectType,
	tags,
	providedFilters: combinedProvidedFilters,
	search,
	persistentQueryParams: ['i', 'ai', 'shi', 'sid', 'wid', 'from', 'src', 'cfcat'],
	getExtraQueryParams: () => ({
		src: source.value === 'curseforge' ? 'curseforge' : undefined,
		sid: serverIdQuery.value || undefined,
		wid: effectiveServerWorldId.value || undefined,
		ai: instanceHideInstalled.value ? 'true' : undefined,
		shi: serverHideInstalled.value ? 'true' : undefined,
	}),
})

watch(source, () => {
	searchState.currentPage.value = 1
	searchState.refreshSearch()
})

watch(
	() => route.query.cfcat,
	() => {
		if (source.value === 'curseforge') searchState.refreshSearch()
	},
)

const showFeaturedSections = computed(
	() => projectType.value === 'modpack' && !searchState.query.value && source.value === 'modrinth',
)
const showCfFeaturedSections = computed(
	() =>
		projectType.value === 'modpack' && !searchState.query.value && source.value === 'curseforge',
)

// ── Daily-rotating featured modpacks ────────────────────────────────────────

const FEATURED_POOL = [
	'fabulously-optimized',
	'adrenaline',
	'better-mc-fabric-bmc2',
	'prominence-2-fabric',
	'cobblemon-fabric',
	'all-the-mods-9',
	'better-mc-forge-bmc4',
	'create-above-and-beyond',
	'the-pixelmon-modpack',
	'rlcraft',
	'sky-factory-4',
	'magic-and-monsters',
]

function seededShuffle<T>(arr: T[], seed: number): T[] {
	const a = [...arr]
	let s = seed
	for (let i = a.length - 1; i > 0; i--) {
		s = (s * 1664525 + 1013904223) & 0x7fffffff
		const j = s % (i + 1)
		;[a[i], a[j]] = [a[j], a[i]]
	}
	return a
}

const todayNum = (() => {
	const d = new Date()
	return d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate()
})()

const dailyShuffled = seededShuffle(FEATURED_POOL, todayNum)
const featuredSlugs = dailyShuffled.slice(0, 3)
const staffPicksSlugs = dailyShuffled.slice(3, 6)

// Dynamic CF featured modpacks — fetched from the CF API when the section is visible
const cfFeaturedMods = ref<CfFeaturedMod[]>([])
const cfPicksMods = ref<CfFeaturedMod[]>([])

watch(
	showCfFeaturedSections,
	async (visible) => {
		if (!visible || cfFeaturedMods.value.length > 0) return
		try {
			const results = await curseforge_search({
				query: null,
				gameVersion: null,
				loader: null,
				categoryId: null,
				classId: 4471,
				page: 0,
				pageSize: 50,
			})
			if (!results?.data?.length) return
			const all: CfFeaturedMod[] = results.data
				.filter((cf: { allowModDistribution?: boolean }) => cf.allowModDistribution !== false)
				.map(
					(cf: {
						id: number
						name: string
						summary: string
						logo?: { thumbnailUrl?: string }
						screenshots?: { url?: string; thumbnailUrl?: string }[]
					}) => ({
						id: cf.id,
						name: cf.name,
						summary: cf.summary,
						logoUrl: cf.logo?.thumbnailUrl ?? null,
						bannerUrl: cf.screenshots?.[0]?.url ?? cf.screenshots?.[0]?.thumbnailUrl ?? null,
					}),
				)
			// Featured: daily-seeded shuffle of top 15 by popularity -> pick 3
			cfFeaturedMods.value = seededShuffle(all.slice(0, 15), todayNum).slice(0, 3)
			// Syl's Picks: positions 15-40 (moderate downloads, not the giants) -> pick 3
			cfPicksMods.value = seededShuffle(all.slice(15, 40), todayNum + 1).slice(0, 3)
		} catch {
			// best-effort, fail silently
		}
	},
	{ immediate: true },
)

const SYL_COMMENTS: Record<string, string> = {
	'fabulously-optimized': 'makes vanilla feel like it was always supposed to run this fast',
	adrenaline: 'bare minimum optimization pack - no excuses not to use it',
	'better-mc-fabric-bmc2': 'vanilla but actually good, hard to go back after this',
	'prominence-2-fabric': 'one of the better kitchen-sink packs right now, genuinely',
	'cobblemon-fabric': 'Pokémon in Minecraft done right - took long enough',
	'all-the-mods-9': 'more content than you will ever actually finish',
	'better-mc-forge-bmc4': 'the Forge version holds up just as well',
	'create-above-and-beyond': 'Create is peak Minecraft and this is the best Create pack',
	'the-pixelmon-modpack': 'the original Pokémon mod, still going strong',
	rlcraft: "genuinely hard - don't say I didn't warn you",
	'sky-factory-4': 'skyblock with automation, you could lose weeks to this',
	'magic-and-monsters': 'covers basically every magic mod worth playing',
}

const SYL_TIPS = [
	'if your modpack crashes on launch, the Fabric API version is usually the culprit',
	'Sodium makes almost everything run better - just saying',
	'always check the changelog before updating a modpack, breaking changes happen more than they should',
	"if a pack feels slow, it's usually one mod hogging RAM, not all of them",
	'when in doubt, allocate 6 GB and see if the stuttering goes away',
]
const CF_SYL_TIPS = [
	'CurseForge modpacks update often - check the changelog before hitting that update button',
	'most CurseForge packs need at least 6 GB RAM - 8 GB is safer for anything ATM-sized',
	'if a mod requires a specific Forge version, the pack version matters as much as the MC version',
	'CurseForge mods with distribution disabled can still be downloaded from CurseForge directly',
	'when something crashes, find the "caused by" line in the log - that\'s almost always the culprit',
]
const sylTipMr = SYL_TIPS[Math.floor(Math.random() * SYL_TIPS.length)]
const sylTipCf = CF_SYL_TIPS[Math.floor(Math.random() * CF_SYL_TIPS.length)]
const sylTip = computed(() => (source.value === 'curseforge' ? sylTipCf : sylTipMr))

const browsePageStyle = computed(() =>
	source.value === 'curseforge'
		? {
				'--source-accent': '#e07428',
				'--color-brand': '#e07428',
				'--color-brand-highlight': '#e07428',
				'--color-button-text-selected': 'rgba(0, 0, 0, 0.85)',
			}
		: {
				'--source-accent': '#1bd96a',
				'--color-brand': '#1bd96a',
				'--color-brand-highlight': '#1bd96a',
				'--color-button-text-selected': 'rgba(0, 0, 0, 0.85)',
			},
)

watch(
	[
		() => searchState.query.value,
		() => searchState.currentFilters.value,
		() => searchState.serverCurrentFilters.value,
		() => projectType.value,
	],
	() => {
		if (isServerContext.value) {
			syncHiddenServerContentProjectIds()
		} else if (instance.value) {
			syncHiddenInstanceProjectIds()
		}
	},
	{ deep: true },
)

watch(queuedServerInstallCount, (count) => {
	if (count === 0) {
		hideSelectedServerInstalls.value = false
	}
})

if (instance.value?.game_version) {
	const gv = instance.value.game_version
	const alreadyHasGv = searchState.serverCurrentFilters.value.some(
		(f) => f.type === 'server_game_version' && f.option === gv,
	)
	if (!alreadyHasGv) {
		searchState.serverCurrentFilters.value.push({ type: 'server_game_version', option: gv })
	}
}

await searchState.refreshSearch()

type UnlistenFn = () => void

let isUnmounted = false
let unlistenProfiles: UnlistenFn | null = null

onMounted(() => {
	profile_listener(async (event: { event: string; profile_path_id: string }) => {
		if (
			instance.value &&
			event.profile_path_id === instance.value.path &&
			event.event === 'synced'
		) {
			await refreshInstalledProjectIds()
			await searchState.refreshSearch()
		}
	})
		.then((unlisten) => {
			if (isUnmounted) {
				unlisten()
				return
			}

			unlistenProfiles = unlisten
		})
		.catch(handleError)
})

onUnmounted(() => {
	isUnmounted = true
	unlistenProfiles?.()
})

function getProjectBrowseQuery() {
	if (!installContext.value) return undefined
	return {
		...route.query,
		b: route.fullPath,
	}
}

provideBrowseManager({
	tags,
	projectType,
	...searchState,
	getProjectLink: (result: Labrinth.Search.v2.ResultSearchProject) => {
		const r = result as { _source?: string; _cfModId?: number }
		if (r._source === 'curseforge') {
			if (!r._cfModId) return ''
			return {
				path: `/cf-project/${r._cfModId}`,
				query: getProjectBrowseQuery(),
			}
		}
		return {
			path: `/project/${result.project_id ?? result.slug}`,
			query: getProjectBrowseQuery(),
		}
	},
	getServerProjectLink: (result: Labrinth.Search.v3.ResultSearchProject) => ({
		path: `/project/${result.slug ?? result.project_id}`,
		query: getProjectBrowseQuery(),
	}),
	selectableProjectTypes,
	showProjectTypeTabs: computed(() => !isServerContext.value),
	variant: 'app',
	getCardActions,
	installContext,
	providedFilters: combinedProvidedFilters,
	hideInstalled: computed({
		get: () => (isServerContext.value ? serverHideInstalled.value : instanceHideInstalled.value),
		set: (val: boolean) => {
			if (isServerContext.value) {
				serverHideInstalled.value = val
				if (val) syncHiddenServerContentProjectIds()
			} else {
				instanceHideInstalled.value = val
				if (val) syncHiddenInstanceProjectIds()
			}
		},
	}),
	showHideInstalled: computed(
		() => (isServerContext.value && projectType.value !== 'modpack') || !!instance.value,
	),
	hideInstalledLabel: computed(() =>
		formatMessage(
			isFromWorlds.value ? messages.hideAddedServers : commonMessages.hideInstalledContentLabel,
		),
	),
	hideSelected: hideSelectedServerInstalls,
	showHideSelected: computed(
		() =>
			isServerContext.value &&
			projectType.value !== 'modpack' &&
			queuedServerInstallCount.value > 0,
	),
	hideSelectedLabel: computed(() => formatMessage(commonMessages.hideSelectedContentLabel)),
	onInstalled: onSearchResultInstalled,
	serverPings,
	getServerModpackContent,
	onContextMenu: (
		event: MouseEvent,
		result: Labrinth.Search.v2.ResultSearchProject | Labrinth.Search.v3.ResultSearchProject,
	) => {
		if ((result as { _source?: string })._source === 'curseforge') return
		handleRightClick(event, result as Labrinth.Search.v3.ResultSearchProject)
	},
	offline,
	lockedFilterMessages,
})
</script>

<template>
	<div class="browse-page flex flex-col gap-3 p-6" :style="browsePageStyle">
		<p v-if="!instance" class="syl-tip">{{ sylTip }}</p>
		<div v-if="showFeaturedSections" class="flex flex-col gap-3">
			<BrowseFeaturedSection :slugs="featuredSlugs" label="Featured" />
			<BrowseFeaturedSection
				:slugs="staffPicksSlugs"
				label="Syl's Picks"
				subtitle="Curated by Syl"
				:comments="SYL_COMMENTS"
			/>
			<hr class="browse-section-divider" />
		</div>
		<div v-if="showCfFeaturedSections" class="flex flex-col gap-3">
			<CfBrowseFeaturedSection :mods="cfFeaturedMods" label="Featured" />
			<CfBrowseFeaturedSection :mods="cfPicksMods" label="Syl's Picks" subtitle="Curated by Syl" />
			<hr class="browse-section-divider" />
		</div>
		<div v-if="showCurseForgeToggle" class="source-toggle">
			<button
				:class="['source-toggle__btn', source === 'modrinth' && 'source-toggle__btn--active-mr']"
				@click="source = 'modrinth'"
			>
				Modrinth
			</button>
			<button
				v-tooltip="cfBetaTooltip"
				:class="['source-toggle__btn', source === 'curseforge' && 'source-toggle__btn--active-cf']"
				@click="source = 'curseforge'"
			>
				CurseForge<span class="source-toggle__beta">Beta</span>
			</button>
		</div>
		<BrowsePageLayout>
			<template #after>
				<ContextMenu ref="contextMenuRef" @option-clicked="handleOptionsClick">
					<template #open_link>
						<GlobeIcon /> {{ formatMessage(commonMessages.openInModrinthButton) }} <ExternalIcon />
					</template>
					<template #copy_link>
						<ClipboardCopyIcon /> {{ formatMessage(commonMessages.copyLinkButton) }}
					</template>
				</ContextMenu>
			</template>
		</BrowsePageLayout>
		<Teleport to="#sidebar-teleport-target">
			<BrowseSidebar />
		</Teleport>

		<Teleport to="body">
			<div v-if="cfPicker" class="cf-picker-overlay" @click.self="cfPicker = null">
				<div class="cf-picker">
					<div class="cf-picker__header">
						<img
							v-if="cfPicker.iconUrl"
							:src="cfPicker.iconUrl"
							class="cf-picker__mod-icon"
							alt=""
						/>
						<div class="cf-picker__mod-info">
							<p class="cf-picker__mod-title">{{ cfPicker.title }}</p>
							<p class="cf-picker__mod-subtitle">Select an instance to install to</p>
						</div>
						<button class="cf-picker__close" @click="cfPicker = null">✕</button>
					</div>
					<div v-if="cfPickerInstances.length === 0" class="cf-picker__empty">
						No installed instances found. Create one first.
					</div>
					<div v-else class="cf-picker__list">
						<button
							v-for="inst in cfPickerInstances"
							:key="inst.path"
							class="cf-picker__instance"
							@click="cfPickerInstall(inst.path, inst.loader, inst.gameVersion)"
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
								<span class="cf-picker__instance-meta"
									>{{ inst.loader }} {{ inst.gameVersion }}</span
								>
							</div>
						</button>
					</div>
				</div>
			</div>
		</Teleport>

		<CreationFlowModal
			v-if="isServerContext && projectType === 'modpack'"
			ref="serverSetupModalRef"
			:type="serverFlowFrom === 'reset-server' ? 'reset-server' : 'server-onboarding'"
			:available-loaders="['vanilla', 'fabric', 'neoforge', 'forge', 'quilt', 'paper', 'purpur']"
			:show-snapshot-toggle="true"
			:on-back="onServerFlowBack"
			:search-modpacks="searchServerModpacks"
			:get-project-versions="getServerProjectVersions"
			:get-loader-manifest="getLoaderManifest"
			@hide="() => {}"
			@browse-modpacks="() => {}"
			@create="handleServerModpackFlowCreate"
		/>
	</div>
</template>

<style lang="scss">
.syl-tip {
	margin: 0;
	font-size: 0.8125rem;
	color: #6b6485;
	font-style: italic;
}

.source-toggle {
	display: flex;
	gap: 0.25rem;
	padding: 0.25rem;
	background: var(--color-surface-3, rgba(142, 132, 207, 0.08));
	border-radius: 0.75rem;
	width: fit-content;

	&__btn {
		padding: 0.35rem 0.9rem;
		border-radius: 0.5rem;
		border: none;
		background: transparent;
		color: var(--color-secondary, #9e98b8);
		font-size: 0.8125rem;
		font-weight: 600;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s;

		&:hover:not(.source-toggle__btn--active-mr):not(.source-toggle__btn--active-cf) {
			background: rgba(142, 132, 207, 0.12);
			color: var(--color-primary, #c1bed1);
		}

		&--active-mr {
			background: #1bd96a;
			color: rgba(0, 0, 0, 0.85);
		}

		&--active-cf {
			background: #e07428;
			color: rgba(0, 0, 0, 0.85);
		}
	}

	&__beta {
		display: inline-block;
		margin-left: 0.375rem;
		padding: 0.0625rem 0.375rem;
		border-radius: 0.375rem;
		background: rgba(0, 0, 0, 0.18);
		font-size: 0.625rem;
		font-weight: 700;
		letter-spacing: 0.02em;
		text-transform: uppercase;
		vertical-align: middle;
	}
}

.browse-page {
	.browse-section-divider {
		border: none;
		border-top: 1px solid var(--color-divider, rgba(193, 190, 209, 0.1));
		margin: 0.125rem 0;
	}

	/* Tint tag chips with faint violet background */
	.grid-project-card-list__tags {
		--_bg-color: rgba(142, 132, 207, 0.1);
		--_color: #b8b0e8;
	}

	/* More breathing room between search result rows */
	.search [role='list'] {
		gap: 0.875rem;
	}

	/* Source-accent hover border on all search result cards */
	.smart-clickable:hover .project-card-container > div {
		border-color: var(--source-accent);
	}

	/* NavTabs active slider and SSR fallback — solid source-accent with dark readable text */
	nav .bg-button-bgSelected {
		background-color: var(--source-accent) !important;
	}
	nav .text-button-textSelected {
		color: rgba(0, 0, 0, 0.85) !important;
	}

	/* Pagination active page number — dark text beats the !text-brand override */
	.page-number-container:not(.page-number) :is(a, button) {
		color: rgba(0, 0, 0, 0.85) !important;
	}
}

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
		border-radius: 0.4rem;
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
		color: var(--color-base, #e2dff0);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	&__mod-subtitle {
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-secondary, #9e98b8);
	}

	&__close {
		background: none;
		border: none;
		color: var(--color-secondary, #9e98b8);
		cursor: pointer;
		font-size: 1rem;
		padding: 0.25rem;
		flex-shrink: 0;
		line-height: 1;

		&:hover {
			color: var(--color-base, #e2dff0);
		}
	}

	&__empty {
		padding: 2rem 1rem;
		text-align: center;
		color: var(--color-secondary, #9e98b8);
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
		color: var(--color-base, #e2dff0);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	&__instance-meta {
		font-size: 0.75rem;
		color: var(--color-secondary, #9e98b8);
		text-transform: capitalize;
	}
}
</style>
