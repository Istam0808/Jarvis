<script lang="ts">
    import { onMount } from "svelte"
    import { invoke } from "@tauri-apps/api/core"
    import { translations, translate } from "@/stores"

    $: t = (key: string) => translate($translations, key)

    let authorName = ""

    const currentYear = new Date().getFullYear()

    onMount(async () => {
        try {
            authorName = await invoke<string>("get_author_name")
        } catch (err) {
            console.error("failed to get author name:", err)
        }
    })
</script>

<footer id="footer">
    <p>© {currentYear}. {t('footer-author')}: <b>{authorName}</b></p>
</footer>

<style lang="scss">
    #footer {
        text-align: center;
        color: #6c6e71;
        font-size: 13px;
        font-weight: normal;
        line-height: 1.7em;
        margin-top: 15px;

        p {
            margin: 0;
            padding: 0;
        }
    }
</style>
