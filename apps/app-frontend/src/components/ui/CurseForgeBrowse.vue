<script setup lang="ts">
import { CheckIcon, PlusIcon, SpinnerIcon } from '@modrinth/assets'
import { injectNotificationManager } from '@modrinth/ui'
import { computed, onMounted, ref, watch } from 'vue'
import {
	curseforge_get_categories,
	curseforge_install_mod,
	curseforge_search,
} from '@/helpers/curseforge.js'

// ── Types ──────────────────────────────────────────────────────────────────────

interface CfCategory {
	id: number
	name: string
	slug: string
	iconUrl?: string
	isClass?: boolean
	classId?: number
	parentCategoryId?: number
}

interface CfMod {
	id: number
	name: string
	slug: string
	summary: string
	downloadCount: number
	categories: CfCategory[]
	classId?: number
	authors: { id: number; name: string }[]
	logo?: { thumbnailUrl?: string; url?: string }
	links?: { websiteUrl?: string }
	latestFilesIndexes: {
		gameVersion: string
		fileId: number
		filename: string
		releaseType: number
		modLoader?: number
	}[]
	dateModified: string
	gamePopularityRank?: number
	isAvailable: boolean
	allowModDistribution?: boolean
}

interface CfSearchResult {
	data: CfMod[]
	pagination: {
		index: number
		pageSize: number
		resultCount: number
		totalCount: number
	}
}

// ── Props ──────────────────────────────────────────────────────────────────────

const props = defineProps<{
	projectType: string
	instance?: {
		path: string
		game_version: string
		loader: string
		name: string
	} | null
}>()

const emit = defineEmits<{
	installed: [modId: number]
}>()

// ── Helpers ────────────────────────────────────────────────────────────────────

const { handleError } = injectNotificationManager()

const CLASS_IDS: Record<string, number> = {
	mod: 6,
	modpack: 4471,
	resourcepack: 12,
	shader: 6552,
}

const classId = computed(() => CLASS_IDS[props.projectType] ?? 6)

// Only send loader filter for mods (resource packs / shaders are loader-agnostic)
const effectiveLoader = computed(() =>
	props.projectType === 'mod' ? props.instance?.loader ?? null : null,
)

function formatDownloads(n: number) {
	if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
	if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`
	return n.toString()
}

// ── State ──────────────────────────────────────────────────────────────────────

const query = ref('')
const selectedCategory = ref<number | undefined>(undefined)
const categories = ref<CfCategory[]>([])
const results = ref<CfMod[]>([])
const loading = ref(false)
const page = ref(0)
const totalCount = ref(0)
const PAGE_SIZE = 20

const installingIds = ref(new Set<number>())
const installedIds = ref(new Set<number>())

const pageCount = computed(() => Math.ceil(totalCount.value / PAGE_SIZE))
const hasNextPage = computed(() => page.value < pageCount.value - 1)
const hasPrevPage = computed(() => page.value > 0)

// ── Data loading ───────────────────────────────────────────────────────────────

async function loadCategories() {
	try {
		categories.value = await curseforge_get_categories(classId.value)
	} catch (e) {
		handleError(e as Error)
	}
}

async function search() {
	loading.value = true
	try {
		const res = (await curseforge_search({
			query: query.value || null,
			gameVersion: props.instance?.game_version ?? null,
			loader: effectiveLoader.value,
			categoryId: selectedCategory.value ?? null,
			classId: classId.value,
			page: page.value,
			pageSize: PAGE_SIZE,
		})) as CfSearchResult

		results.value = res.data
		totalCount.value = res.pagination.totalCount
	} catch (e) {
		handleError(e as Error)
		results.value = []
		totalCount.value = 0
	} finally {
		loading.value = false
	}
}

// ── Install ────────────────────────────────────────────────────────────────────

async function install(mod: CfMod) {
	if (!props.instance) return

	installingIds.value = new Set([...installingIds.value, mod.id])
	try {
		await curseforge_install_mod({
			profilePath: props.instance.path,
			modId: mod.id,
			gameVersion: props.instance.game_version,
			loader: effectiveLoader.value,
			projectType: props.projectType,
		})
		installedIds.value = new Set([...installedIds.value, mod.id])
		emit('installed', mod.id)
	} catch (e) {
		handleError(e as Error)
	} finally {
		installingIds.value = new Set(
			[...installingIds.value].filter((id) => id !== mod.id),
		)
	}
}

// ── Watchers & lifecycle ───────────────────────────────────────────────────────

let debounceTimer: ReturnType<typeof setTimeout> | null = null

watch(query, () => {
	page.value = 0
	if (debounceTimer) clearTimeout(debounceTimer)
	debounceTimer = setTimeout(search, 400)
})

watch([selectedCategory, classId], () => {
	page.value = 0
	search()
})

function goToPage(n: number) {
	page.value = n
	search()
}

onMounted(() => {
	loadCategories()
	search()
})
</script>

<template>
	<div class="cf-browse flex gap-4">
		<!-- Sidebar -->
		<aside class="cf-sidebar flex flex-col gap-3">
			<div class="cf-search-box">
				<input
					v-model="query"
					class="cf-input"
					placeholder="Search CurseForge…"
					type="search"
				/>
			</div>

			<div class="cf-filter-group">
				<p class="cf-filter-label">Category</p>
				<button
					class="cf-filter-opt"
					:class="{ active: selectedCategory === undefined }"
					@click="selectedCategory = undefined"
				>
					All
				</button>
				<button
					v-for="cat in categories"
					:key="cat.id"
					class="cf-filter-opt"
					:class="{ active: selectedCategory === cat.id }"
					@click="selectedCategory = cat.id"
				>
					{{ cat.name }}
				</button>
			</div>
		</aside>

		<!-- Results -->
		<div class="cf-results flex flex-col gap-3 min-w-0 flex-1">
			<!-- Loading spinner -->
			<div v-if="loading" class="cf-spinner">
				<SpinnerIcon class="animate-spin" />
			</div>

			<!-- No results -->
			<p v-else-if="results.length === 0" class="cf-empty">
				No CurseForge results found.
			</p>

			<!-- Result cards -->
			<template v-else>
				<div
					v-for="mod in results"
					:key="mod.id"
					class="cf-card"
				>
					<img
						v-if="mod.logo?.thumbnailUrl"
						class="cf-card__logo"
						:src="mod.logo.thumbnailUrl"
						:alt="mod.name"
						loading="lazy"
					/>
					<div v-else class="cf-card__logo cf-card__logo--placeholder" />

					<div class="cf-card__body min-w-0 flex-1">
						<div class="cf-card__header">
							<span class="cf-card__name">{{ mod.name }}</span>
							<span class="cf-card__author">
								{{ mod.authors[0]?.name }}
							</span>
						</div>
						<p class="cf-card__summary">{{ mod.summary }}</p>
						<div class="cf-card__meta">
							<span class="cf-card__downloads">
								⬇ {{ formatDownloads(mod.downloadCount) }}
							</span>
							<span
								v-for="cat in mod.categories.slice(0, 3)"
								:key="cat.id"
								class="cf-tag"
							>
								{{ cat.name }}
							</span>
						</div>
					</div>

					<!-- Install button (only when in instance context) -->
					<div v-if="instance" class="cf-card__action">
						<button
							class="cf-install-btn"
							:disabled="
								installedIds.has(mod.id) ||
								installingIds.has(mod.id) ||
								mod.allowModDistribution === false
							"
							:title="
								mod.allowModDistribution === false
									? 'Author has disabled distribution'
									: undefined
							"
							@click="install(mod)"
						>
							<SpinnerIcon
								v-if="installingIds.has(mod.id)"
								class="animate-spin"
							/>
							<CheckIcon v-else-if="installedIds.has(mod.id)" />
							<PlusIcon v-else />
							<span>
								{{
									installingIds.has(mod.id)
										? 'Installing…'
										: installedIds.has(mod.id)
											? 'Installed'
											: mod.allowModDistribution === false
												? 'Unavailable'
												: 'Install'
								}}
							</span>
						</button>
					</div>
				</div>

				<!-- Pagination -->
				<div v-if="pageCount > 1" class="cf-pagination">
					<button
						class="cf-page-btn"
						:disabled="!hasPrevPage"
						@click="goToPage(page - 1)"
					>
						← Prev
					</button>
					<span class="cf-page-info">
						Page {{ page + 1 }} of {{ pageCount }}
					</span>
					<button
						class="cf-page-btn"
						:disabled="!hasNextPage"
						@click="goToPage(page + 1)"
					>
						Next →
					</button>
				</div>
			</template>
		</div>
	</div>
</template>

<style lang="scss" scoped>
.cf-browse {
	width: 100%;
}

// ── Sidebar ────────────────────────────────────────────────────────────────────

.cf-sidebar {
	width: 11rem;
	flex-shrink: 0;
}

.cf-input {
	width: 100%;
	padding: 0.4rem 0.6rem;
	border-radius: 0.375rem;
	border: 1px solid var(--color-button-bg, rgba(142, 132, 207, 0.2));
	background: var(--color-raised-bg, rgba(142, 132, 207, 0.06));
	color: var(--color-base, #d0cce9);
	font-size: 0.8125rem;
	outline: none;

	&:focus {
		border-color: var(--color-brand, #8e84cf);
	}

	&::placeholder {
		color: var(--color-secondary, #6b6485);
	}
}

.cf-filter-label {
	font-size: 0.75rem;
	font-weight: 600;
	text-transform: uppercase;
	letter-spacing: 0.04em;
	color: var(--color-secondary, #6b6485);
	margin: 0 0 0.25rem;
}

.cf-filter-opt {
	display: block;
	width: 100%;
	text-align: left;
	padding: 0.3rem 0.5rem;
	border-radius: 0.25rem;
	font-size: 0.8125rem;
	color: var(--color-base, #d0cce9);
	background: transparent;
	border: none;
	cursor: pointer;

	&:hover {
		background: rgba(142, 132, 207, 0.12);
	}

	&.active {
		background: rgba(142, 132, 207, 0.2);
		color: var(--color-brand, #b8b0e8);
		font-weight: 600;
	}
}

// ── Results ────────────────────────────────────────────────────────────────────

.cf-spinner {
	display: flex;
	justify-content: center;
	padding: 3rem 0;
	color: var(--color-secondary, #6b6485);
	font-size: 1.5rem;
}

.cf-empty {
	text-align: center;
	color: var(--color-secondary, #6b6485);
	padding: 3rem 0;
	font-size: 0.875rem;
}

.cf-card {
	display: flex;
	align-items: flex-start;
	gap: 0.75rem;
	padding: 0.75rem;
	border-radius: 0.5rem;
	background: var(--color-raised-bg, rgba(142, 132, 207, 0.06));
	border: 1px solid var(--color-button-bg, rgba(142, 132, 207, 0.12));
	transition: border-color 0.15s;

	&:hover {
		border-color: rgba(142, 132, 207, 0.3);
	}
}

.cf-card__logo {
	width: 3rem;
	height: 3rem;
	border-radius: 0.375rem;
	object-fit: cover;
	flex-shrink: 0;

	&--placeholder {
		background: rgba(142, 132, 207, 0.12);
	}
}

.cf-card__header {
	display: flex;
	align-items: baseline;
	gap: 0.5rem;
	margin-bottom: 0.2rem;
}

.cf-card__name {
	font-size: 0.9rem;
	font-weight: 600;
	color: var(--color-base, #d0cce9);
}

.cf-card__author {
	font-size: 0.75rem;
	color: var(--color-secondary, #6b6485);
}

.cf-card__summary {
	font-size: 0.8rem;
	color: var(--color-secondary, #8c87ab);
	margin: 0 0 0.35rem;
	display: -webkit-box;
	-webkit-line-clamp: 2;
	-webkit-box-orient: vertical;
	overflow: hidden;
}

.cf-card__meta {
	display: flex;
	align-items: center;
	flex-wrap: wrap;
	gap: 0.375rem;
}

.cf-card__downloads {
	font-size: 0.75rem;
	color: var(--color-secondary, #6b6485);
}

.cf-tag {
	font-size: 0.7rem;
	padding: 0.1rem 0.4rem;
	border-radius: 0.25rem;
	background: rgba(142, 132, 207, 0.1);
	color: var(--color-brand, #b8b0e8);
}

.cf-card__action {
	flex-shrink: 0;
	display: flex;
	align-items: center;
}

.cf-install-btn {
	display: flex;
	align-items: center;
	gap: 0.3rem;
	padding: 0.35rem 0.7rem;
	border-radius: 0.375rem;
	border: none;
	background: var(--color-brand, #8e84cf);
	color: rgba(0, 0, 0, 0.85);
	font-size: 0.8rem;
	font-weight: 600;
	cursor: pointer;
	transition: opacity 0.15s;
	white-space: nowrap;

	svg {
		width: 0.9rem;
		height: 0.9rem;
	}

	&:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	&:not(:disabled):hover {
		opacity: 0.85;
	}
}

// ── Pagination ─────────────────────────────────────────────────────────────────

.cf-pagination {
	display: flex;
	align-items: center;
	justify-content: center;
	gap: 1rem;
	padding-top: 0.5rem;
}

.cf-page-btn {
	padding: 0.3rem 0.75rem;
	border-radius: 0.375rem;
	border: 1px solid var(--color-button-bg, rgba(142, 132, 207, 0.2));
	background: var(--color-raised-bg, rgba(142, 132, 207, 0.06));
	color: var(--color-base, #d0cce9);
	font-size: 0.8rem;
	cursor: pointer;

	&:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	&:not(:disabled):hover {
		background: rgba(142, 132, 207, 0.12);
	}
}

.cf-page-info {
	font-size: 0.8rem;
	color: var(--color-secondary, #6b6485);
}
</style>
