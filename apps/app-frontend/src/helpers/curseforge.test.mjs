import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
	curseforge_search,
	isCurseForgeInstallPaused,
	onCurseForgeInstallPaused,
	parseCurseForgeSearchParams,
	resolveCurseForgePackInstall,
} from './curseforge.js'

test('paused installs notify recovery without pretending an instance was installed', () => {
	const pending = { id: '12-34', downloadedFiles: 4, missingFiles: [{ fileId: 5 }] }
	const events = []
	const unsubscribe = onCurseForgeInstallPaused((job) => events.push(job))
	try {
		assert.throws(
			() => resolveCurseForgePackInstall({ profilePath: null, pending }),
			(error) => isCurseForgeInstallPaused(error) && error.pending === pending,
		)
		assert.deepEqual(events, [pending])
		assert.equal(
			resolveCurseForgePackInstall({ profilePath: 'Ready instance', pending: null }),
			'Ready instance',
		)
		assert.deepEqual(events, [pending])
	} finally {
		unsubscribe()
	}
})

test('malformed installation replies cannot be used as an instance path', () => {
	for (const value of [null, {}, { profilePath: '' }, { pending: {} }]) {
		assert.throws(() => resolveCurseForgePackInstall(value), /invalid installation result/)
	}
	assert.equal(isCurseForgeInstallPaused(new Error('HTTP 403')), false)
})

test('search text is decoded once and keeps literal percent and plus characters', () => {
	const params = new URLSearchParams({ query: '100% + fun', limit: '100', offset: '100' })
	assert.deepEqual(parseCurseForgeSearchParams(`?${params}`), {
		query: '100% + fun',
		page: 1,
		pageSize: 100,
		sort: null,
	})
})

test('invalid pagination cannot silently change the result offset', () => {
	for (const params of ['?limit=0', '?limit=101', '?offset=-1', '?limit=20&offset=21']) {
		assert.throws(() => parseCurseForgeSearchParams(params), /Invalid CurseForge search page/)
	}
})

test('selected version, loader and sort filters reach the Tauri command', async () => {
	const requests = []
	const previous = globalThis.window
	globalThis.window = {
		__TAURI_INTERNALS__: {
			invoke: async (command, args) => {
				requests.push({ command, args })
				return { data: [], pagination: { totalCount: 0 } }
			},
		},
	}
	try {
		await curseforge_search({
			...parseCurseForgeSearchParams('?limit=100&offset=100&index=downloads'),
			classId: 6,
			gameVersions: ['1.20.1', '1.21.1'],
			loaders: ['fabric', 'forge'],
		})
		assert.equal(requests[0].command, 'plugin:curseforge|curseforge_search')
		assert.deepEqual(requests[0].args.gameVersions, ['1.20.1', '1.21.1'])
		assert.deepEqual(requests[0].args.loaders, ['fabric', 'forge'])
		assert.equal(requests[0].args.sort, 'downloads')
		assert.equal(requests[0].args.page, 1)
		assert.equal(requests[0].args.pageSize, 100)
	} finally {
		if (previous === undefined) delete globalThis.window
		else globalThis.window = previous
	}
})
