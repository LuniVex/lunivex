<script setup>
import { SearchIcon } from '@modrinth/assets'
import {
	Accordion,
	DropdownSelect,
	formatLoader,
	StyledInput,
	useVIntl,
} from '@modrinth/ui'
import { useStorage } from '@vueuse/core'
import dayjs from 'dayjs'
import { computed, ref } from 'vue'

import Instance from '@/components/ui/Instance.vue'

const { formatMessage } = useVIntl()

const props = defineProps({
	instances: {
		type: Array,
		default() {
			return []
		},
	},
	label: {
		type: String,
		default: '',
	},
})

const state = useStorage(
	`${props.label}-grid-display-state`,
	{
		group: 'Group',
		sortBy: 'Name',
		collapsedGroups: [],
	},
	localStorage,
	{ mergeDefaults: true },
)

const search = ref('')
const collapsedSectionKeys = computed(() => new Set(state.value.collapsedGroups ?? []))

const getSectionKey = (sectionName) => `${state.value.group}:${sectionName}`

const isSectionCollapsed = (sectionName) => {
	return collapsedSectionKeys.value.has(getSectionKey(sectionName))
}

const setSectionCollapsed = (sectionName, collapsed) => {
	const sectionKey = getSectionKey(sectionName)
	const collapsedSections = new Set(state.value.collapsedGroups ?? [])

	if (collapsed) {
		collapsedSections.add(sectionKey)
	} else {
		collapsedSections.delete(sectionKey)
	}

	state.value.collapsedGroups = [...collapsedSections]
}

const filteredResults = computed(() => {
	const { group = 'Group', sortBy = 'Name' } = state.value

	const instances = props.instances.filter((instance) => {
		return instance.name.toLowerCase().includes(search.value.toLowerCase())
	})

	if (sortBy === 'Name') {
		instances.sort((a, b) => {
			return a.name.localeCompare(b.name)
		})
	}

	if (sortBy === 'Game version') {
		instances.sort((a, b) => {
			return a.game_version.localeCompare(b.game_version, undefined, { numeric: true })
		})
	}

	if (sortBy === 'Last played') {
		instances.sort((a, b) => {
			return dayjs(b.last_played ?? 0).diff(dayjs(a.last_played ?? 0))
		})
	}

	if (sortBy === 'Date created') {
		instances.sort((a, b) => {
			return dayjs(b.date_created).diff(dayjs(a.date_created))
		})
	}

	if (sortBy === 'Date modified') {
		instances.sort((a, b) => {
			return dayjs(b.date_modified).diff(dayjs(a.date_modified))
		})
	}

	const instanceMap = new Map()

	if (group === 'Loader') {
		instances.forEach((instance) => {
			const loader = formatLoader(formatMessage, instance.loader)
			if (!instanceMap.has(loader)) {
				instanceMap.set(loader, [])
			}

			instanceMap.get(loader).push(instance)
		})
	} else if (group === 'Game version') {
		instances.forEach((instance) => {
			if (!instanceMap.has(instance.game_version)) {
				instanceMap.set(instance.game_version, [])
			}

			instanceMap.get(instance.game_version).push(instance)
		})
	} else if (group === 'Group') {
		instances.forEach((instance) => {
			if (instance.groups.length === 0) {
				instance.groups.push('None')
			}

			for (const category of instance.groups) {
				if (!instanceMap.has(category)) {
					instanceMap.set(category, [])
				}

				instanceMap.get(category).push(instance)
			}
		})
	} else {
		return instanceMap.set('None', instances)
	}

	// For 'name', we intuitively expect the sorting to apply to the name of the group first, not just the name of the instance
	// ie: Category A should come before B, even if the first instance in B comes before the first instance in A
	if (sortBy === 'Name') {
		const sortedEntries = [...instanceMap.entries()].sort((a, b) => {
			// None should always be first
			if (a[0] === 'None' && b[0] !== 'None') {
				return -1
			}
			if (a[0] !== 'None' && b[0] === 'None') {
				return 1
			}
			return a[0].localeCompare(b[0])
		})
		instanceMap.clear()
		sortedEntries.forEach((entry) => {
			instanceMap.set(entry[0], entry[1])
		})
	}
	// default sorting would do 1.20.4 < 1.8.9 because 2 < 8
	// localeCompare with numeric=true puts 1.8.9 < 1.20.4 because 8 < 20
	if (group === 'Game version') {
		const sortedEntries = [...instanceMap.entries()].sort((a, b) => {
			return a[0].localeCompare(b[0], undefined, { numeric: true })
		})
		instanceMap.clear()
		sortedEntries.forEach((entry) => {
			instanceMap.set(entry[0], entry[1])
		})
	}

	return instanceMap
})
</script>
<template>
	<div class="flex gap-2">
		<StyledInput
			v-model="search"
			:icon="SearchIcon"
			type="text"
			placeholder="Search"
			clearable
			wrapper-class="flex-1"
		/>
		<DropdownSelect
			v-slot="{ selected }"
			v-model="state.sortBy"
			name="Sort Dropdown"
			class="max-w-[16rem]"
			:options="['Name', 'Last played', 'Date created', 'Date modified', 'Game version']"
			placeholder="Select..."
		>
			<span class="font-semibold text-primary">Sort by: </span>
			<span class="font-semibold text-secondary">{{ selected }}</span>
		</DropdownSelect>
		<DropdownSelect
			v-slot="{ selected }"
			v-model="state.group"
			class="max-w-[16rem]"
			name="Group Dropdown"
			:options="['Group', 'Loader', 'Game version', 'None']"
			placeholder="Select..."
		>
			<span class="font-semibold text-primary">Group by: </span>
			<span class="font-semibold text-secondary">{{ selected }}</span>
		</DropdownSelect>
	</div>
	<Accordion
		v-for="instanceSection in Array.from(filteredResults, ([key, value]) => ({
			key,
			value,
		}))"
		:key="instanceSection.key"
		:divider="instanceSection.key !== 'None'"
		:open-by-default="!isSectionCollapsed(instanceSection.key)"
		class="row"
		@on-open="setSectionCollapsed(instanceSection.key, false)"
		@on-close="setSectionCollapsed(instanceSection.key, true)"
	>
		<template v-if="instanceSection.key !== 'None'" #title>
			<span class="text-base">{{ instanceSection.key }}</span>
		</template>
		<section class="instances">
			<Instance
				v-for="instance in instanceSection.value"
				:key="instance.path + instance.install_stage"
				:instance="instance"
			/>
		</section>
	</Accordion>
</template>
<style lang="scss" scoped>
.row {
	width: 100%;
}

.instances {
	display: grid;
	grid-template-columns: repeat(auto-fill, minmax(16rem, 1fr));
	width: 100%;
	gap: 0.75rem;
	margin-right: auto;
	scroll-behavior: smooth;
	overflow-y: auto;
}
</style>
