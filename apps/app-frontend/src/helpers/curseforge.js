import { invoke } from '@tauri-apps/api/core'

const pausedInstallListeners = new Set()

export class CurseForgeInstallPaused extends Error {
	constructor(pending) {
		super('Installation paused. Your downloads are saved; open Saved installs to continue.')
		this.name = 'CurseForgeInstallPaused'
		this.code = 'CURSEFORGE_INSTALL_PAUSED'
		this.pending = pending
	}
}

export function isCurseForgeInstallPaused(error) {
	return error?.code === 'CURSEFORGE_INSTALL_PAUSED'
}

export function onCurseForgeInstallPaused(listener) {
	pausedInstallListeners.add(listener)
	return () => pausedInstallListeners.delete(listener)
}

export function resolveCurseForgePackInstall(result) {
	if (typeof result?.profilePath === 'string' && result.profilePath && !result.pending) {
		return result.profilePath
	}
	if (typeof result?.pending?.id === 'string') {
		for (const listener of pausedInstallListeners) listener(result.pending)
		throw new CurseForgeInstallPaused(result.pending)
	}
	throw new Error('CurseForge returned an invalid installation result')
}

export async function curseforge_pending_installs() {
	return await invoke('plugin:curseforge|curseforge_pending_installs')
}

export async function curseforge_resume_install(jobId) {
	return await invoke('plugin:curseforge|curseforge_resume_install', { jobId })
}

export async function curseforge_import_install_files(jobId, paths) {
	return await invoke('plugin:curseforge|curseforge_import_install_files', { jobId, paths })
}

export async function curseforge_discard_install(jobId) {
	return await invoke('plugin:curseforge|curseforge_discard_install', { jobId })
}

export function parseCurseForgeSearchParams(requestParams) {
	const params = new URLSearchParams(requestParams)
	const pageSize = Number(params.get('limit') ?? 20)
	const offset = Number(params.get('offset') ?? 0)
	if (
		!Number.isInteger(pageSize) ||
		pageSize < 1 ||
		pageSize > 100 ||
		!Number.isInteger(offset) ||
		offset < 0 ||
		offset % pageSize !== 0
	) {
		throw new Error('Invalid CurseForge search page')
	}
	return {
		query: params.get('query'),
		page: offset / pageSize,
		pageSize,
		sort: params.get('index'),
	}
}

export async function curseforge_search({
	query = null,
	gameVersion = null,
	loader = null,
	categoryId = null,
	classId,
	page,
	pageSize,
	gameVersions = null,
	loaders = null,
	sort = null,
}) {
	return await invoke('plugin:curseforge|curseforge_search', {
		query,
		gameVersion,
		loader,
		categoryId,
		classId,
		page,
		pageSize,
		gameVersions,
		loaders,
		sort,
	})
}

export async function curseforge_get_categories(classId) {
	return await invoke('plugin:curseforge|curseforge_get_categories', {
		classId,
	})
}

export async function curseforge_get_mod(modId) {
	return await invoke('plugin:curseforge|curseforge_get_mod', { modId })
}

export async function curseforge_get_mods(modIds) {
	return await invoke('plugin:curseforge|curseforge_get_mods', { modIds })
}

export async function curseforge_get_mod_files(modId, gameVersion, loader, page, pageSize) {
	return await invoke('plugin:curseforge|curseforge_get_mod_files', {
		modId,
		gameVersion,
		loader,
		page,
		pageSize,
	})
}

export async function curseforge_get_file(modId, fileId) {
	return await invoke('plugin:curseforge|curseforge_get_file', { modId, fileId })
}

export async function curseforge_get_file_changelog(modId, fileId) {
	return await invoke('plugin:curseforge|curseforge_get_file_changelog', { modId, fileId })
}

export async function curseforge_install_mod({
	profilePath,
	modId,
	gameVersion,
	loader,
	projectType,
}) {
	return await invoke('plugin:curseforge|curseforge_install_mod', {
		profilePath,
		modId,
		gameVersion,
		loader,
		projectType,
	})
}

export async function curseforge_get_mod_description(modId) {
	const html = await invoke('plugin:curseforge|curseforge_get_mod_description', { modId })
	return stripCfDescriptionExtraField(html)
}

// CurseForge's description endpoint sometimes concatenates a separate,
// author-filled "additional info" field (e.g. asset attribution text) as a
// trailing top-level sibling after the main description content, styled and
// positioned independently of it (confirmed by comparing raw API output
// across multiple mods: only present when that separate field is filled in,
// and always appears after the main content's wrapping <div> closes, never
// embedded within it). Strip it: when the description's first top-level node
// is a wrapping <div> and further top-level siblings follow, drop them and
// keep only that div. Descriptions with no such wrapper (content is already
// flat top-level HTML, as most mods' descriptions are) are left untouched.
export function stripCfDescriptionExtraField(html) {
	if (!html) return html
	const doc = new DOMParser().parseFromString(html, 'text/html')
	const topLevelNodes = Array.from(doc.body.childNodes).filter(
		(node) => !(node.nodeType === Node.TEXT_NODE && !node.textContent.trim()),
	)
	if (topLevelNodes.length < 2) return html
	const first = topLevelNodes[0]
	if (first.nodeType !== Node.ELEMENT_NODE || first.tagName !== 'DIV') return html
	return first.outerHTML
}

export async function curseforge_install_modpack(modId) {
	return resolveCurseForgePackInstall(
		await invoke('plugin:curseforge|curseforge_install_modpack', { modId }),
	)
}

export async function curseforge_install_mod_file({
	profilePath,
	modId,
	fileId,
	fileName,
	projectType,
}) {
	return await invoke('plugin:curseforge|curseforge_install_mod_file', {
		profilePath,
		modId,
		fileId,
		fileName,
		projectType,
	})
}

export async function curseforge_install_modpack_file(modId, fileId) {
	return resolveCurseForgePackInstall(
		await invoke('plugin:curseforge|curseforge_install_modpack_file', { modId, fileId }),
	)
}
