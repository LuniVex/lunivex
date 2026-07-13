import { invoke } from '@tauri-apps/api/core'

export async function curseforge_search({
	query,
	gameVersion,
	loader,
	categoryId,
	classId,
	page,
	pageSize,
}) {
	return await invoke('plugin:curseforge|curseforge_search', {
		query,
		gameVersion,
		loader,
		categoryId,
		classId,
		page,
		pageSize,
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
	return await invoke('plugin:curseforge|curseforge_install_modpack', { modId })
}

export async function curseforge_install_mod_file({ profilePath, modId, fileId, fileName, projectType }) {
	return await invoke('plugin:curseforge|curseforge_install_mod_file', {
		profilePath,
		modId,
		fileId,
		fileName,
		projectType,
	})
}

export async function curseforge_install_modpack_file(modId, fileId) {
	return await invoke('plugin:curseforge|curseforge_install_modpack_file', { modId, fileId })
}
