//! Interface text in several languages. The strings are fixed labels without
//! plurals or grammar, so a static table per language is enough.

use serde::{Deserialize, Serialize};
use std::cell::Cell;

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy, Debug)]
pub enum Language {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-hans")]
    SimplifiedChinese,
    #[serde(rename = "zh-hant")]
    TraditionalChinese,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "ru")]
    Russian,
    #[serde(rename = "es")]
    Spanish,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "de")]
    German,
}

use Language::*;

/// The order the settings menu cycles through.
const ALL: [Language; 10] = [
    Auto,
    English,
    SimplifiedChinese,
    TraditionalChinese,
    Japanese,
    Korean,
    Russian,
    Spanish,
    French,
    German,
];

impl Language {
    pub fn next(self) -> Self {
        ALL[(self.index() + 1) % ALL.len()]
    }

    pub fn prev(self) -> Self {
        ALL[(self.index() + ALL.len() - 1) % ALL.len()]
    }

    fn index(self) -> usize {
        ALL.iter().position(|&l| l == self).unwrap_or(0)
    }

    /// The language's name in itself, as language pickers show it.
    pub fn name(self) -> &'static str {
        match self {
            Auto => tr(Text::Auto),
            English => "English",
            SimplifiedChinese => "简体中文",
            TraditionalChinese => "繁體中文",
            Japanese => "日本語",
            Korean => "한국어",
            Russian => "Русский",
            Spanish => "Español",
            French => "Français",
            German => "Deutsch",
        }
    }
}

thread_local! {
    // The language in use, English until set() runs. The reader has one thread;
    // tests each have their own, so a test switching languages affects no other.
    static CURRENT: Cell<Language> = const { Cell::new(Language::English) };
}

/// Switches the interface language; Auto follows the system locale.
pub fn set(language: Language) {
    let resolved = match language {
        Auto => from_locale(),
        other => other,
    };
    CURRENT.with(|current| current.set(resolved));
}

fn current() -> Language {
    CURRENT.with(Cell::get)
}

/// The first of LC_ALL, LC_MESSAGES and LANG that is set, as POSIX orders them.
fn from_locale() -> Language {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    parse_locale(&locale)
}

/// Reads a locale name such as "zh_TW.UTF-8" or "zh-Hant".
fn parse_locale(locale: &str) -> Language {
    let tag = locale
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .replace('-', "_")
        .to_lowercase();
    let mut parts = tag.split('_');
    match (parts.next(), parts.next()) {
        (Some("zh"), Some("tw" | "hk" | "mo" | "hant")) => TraditionalChinese,
        (Some("zh"), _) => SimplifiedChinese,
        (Some("ja"), _) => Japanese,
        (Some("ko"), _) => Korean,
        (Some("ru"), _) => Russian,
        (Some("es"), _) => Spanish,
        (Some("fr"), _) => French,
        (Some("de"), _) => German,
        _ => English,
    }
}

#[derive(Clone, Copy)]
pub enum Text {
    TableOfContents,
    Settings,
    MainUi,
    Footer,
    MaxWidth,
    MarginLeft,
    MarginRight,
    ScrollLines,
    Theme,
    Language,
    Images,
    Blocks,
    PlainStyles,
    ShowFooter,
    DimFooter,
    FooterAlign,
    ChapterTitle,
    ProgressMode,
    ProgressBar,
    BarLength,
    ProgressPercent,
    ChapterLoc,
    On,
    Off,
    Left,
    Center,
    Right,
    Chapter,
    Overall,
    Auto,
    TerminalTheme,
    Image,
    Section,
    Usage,
    CannotOpen,
    NotZip,
    NotEpub,
    NoChapters,
    TerminalError,
}

/// The text in the current language. Messages may hold {path} and {error}.
pub fn tr(text: Text) -> &'static str {
    match current() {
        SimplifiedChinese => simplified_chinese(text),
        TraditionalChinese => traditional_chinese(text),
        Japanese => japanese(text),
        Korean => korean(text),
        Russian => russian(text),
        Spanish => spanish(text),
        French => french(text),
        German => german(text),
        Auto | English => english(text),
    }
}

fn english(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "Table of Contents",
        Text::Settings => "Settings",
        Text::MainUi => "Main UI",
        Text::Footer => "Footer",
        Text::MaxWidth => "Max Width",
        Text::MarginLeft => "Margin Left",
        Text::MarginRight => "Margin Right",
        Text::ScrollLines => "Scroll Lines",
        Text::Theme => "Theme",
        Text::Language => "Language",
        Text::Images => "Images",
        Text::Blocks => "Blocks",
        Text::PlainStyles => "Plain Styles",
        Text::ShowFooter => "Show Footer",
        Text::DimFooter => "Dim Footer",
        Text::FooterAlign => "Footer Align",
        Text::ChapterTitle => "Chapter Title",
        Text::ProgressMode => "Progress Mode",
        Text::ProgressBar => "Progress Bar",
        Text::BarLength => "Bar Length",
        Text::ProgressPercent => "Progress %",
        Text::ChapterLoc => "Chapter Loc",
        Text::On => "On",
        Text::Off => "Off",
        Text::Left => "Left",
        Text::Center => "Center",
        Text::Right => "Right",
        Text::Chapter => "Chapter",
        Text::Overall => "Overall",
        Text::Auto => "Auto",
        Text::TerminalTheme => "Terminal",
        Text::Image => "Image",
        Text::Section => "Section",
        Text::Usage => "Usage: cargo run -- <path_to_epub>",
        Text::CannotOpen => "cannot open {path}: {error}",
        Text::NotZip => "{path} is not an EPUB (not a zip archive)",
        Text::NotEpub => "{path} is not a readable EPUB (no package or spine)",
        Text::NoChapters => "no chapters found in {path}",
        Text::TerminalError => "terminal error: {error}",
    }
}

fn simplified_chinese(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "目录",
        Text::Settings => "设置",
        Text::MainUi => "主界面",
        Text::Footer => "页脚",
        Text::MaxWidth => "最大宽度",
        Text::MarginLeft => "左边距",
        Text::MarginRight => "右边距",
        Text::ScrollLines => "滚动行数",
        Text::Theme => "主题",
        Text::Language => "语言",
        Text::Images => "图片",
        Text::Blocks => "字符块",
        Text::PlainStyles => "简洁样式",
        Text::ShowFooter => "显示页脚",
        Text::DimFooter => "页脚变暗",
        Text::FooterAlign => "页脚对齐",
        Text::ChapterTitle => "章节标题",
        Text::ProgressMode => "进度模式",
        Text::ProgressBar => "进度条",
        Text::BarLength => "进度条长度",
        Text::ProgressPercent => "进度百分比",
        Text::ChapterLoc => "章节位置",
        Text::On => "开",
        Text::Off => "关",
        Text::Left => "左",
        Text::Center => "居中",
        Text::Right => "右",
        Text::Chapter => "本章",
        Text::Overall => "全书",
        Text::Auto => "自动",
        Text::TerminalTheme => "终端",
        Text::Image => "图片",
        Text::Section => "章节",
        Text::Usage => "用法：cargo run -- <EPUB 文件路径>",
        Text::CannotOpen => "无法打开 {path}：{error}",
        Text::NotZip => "{path} 不是 EPUB 文件（不是 zip 压缩包）",
        Text::NotEpub => "{path} 不是可读取的 EPUB（缺少 package 或 spine）",
        Text::NoChapters => "{path} 中没有找到章节",
        Text::TerminalError => "终端错误：{error}",
    }
}

fn traditional_chinese(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "目錄",
        Text::Settings => "設定",
        Text::MainUi => "主介面",
        Text::Footer => "頁尾",
        Text::MaxWidth => "最大寬度",
        Text::MarginLeft => "左邊距",
        Text::MarginRight => "右邊距",
        Text::ScrollLines => "捲動行數",
        Text::Theme => "主題",
        Text::Language => "語言",
        Text::Images => "圖片",
        Text::Blocks => "字元區塊",
        Text::PlainStyles => "簡潔樣式",
        Text::ShowFooter => "顯示頁尾",
        Text::DimFooter => "頁尾變暗",
        Text::FooterAlign => "頁尾對齊",
        Text::ChapterTitle => "章節標題",
        Text::ProgressMode => "進度模式",
        Text::ProgressBar => "進度列",
        Text::BarLength => "進度列長度",
        Text::ProgressPercent => "進度百分比",
        Text::ChapterLoc => "章節位置",
        Text::On => "開",
        Text::Off => "關",
        Text::Left => "左",
        Text::Center => "置中",
        Text::Right => "右",
        Text::Chapter => "本章",
        Text::Overall => "全書",
        Text::Auto => "自動",
        Text::TerminalTheme => "終端機",
        Text::Image => "圖片",
        Text::Section => "章節",
        Text::Usage => "用法：cargo run -- <EPUB 檔案路徑>",
        Text::CannotOpen => "無法開啟 {path}：{error}",
        Text::NotZip => "{path} 不是 EPUB 檔案（不是 zip 壓縮檔）",
        Text::NotEpub => "{path} 不是可讀取的 EPUB（缺少 package 或 spine）",
        Text::NoChapters => "{path} 中找不到章節",
        Text::TerminalError => "終端機錯誤：{error}",
    }
}

fn japanese(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "目次",
        Text::Settings => "設定",
        Text::MainUi => "メイン画面",
        Text::Footer => "フッター",
        Text::MaxWidth => "最大幅",
        Text::MarginLeft => "左余白",
        Text::MarginRight => "右余白",
        Text::ScrollLines => "スクロール行数",
        Text::Theme => "テーマ",
        Text::Language => "言語",
        Text::Images => "画像",
        Text::Blocks => "ブロック",
        Text::PlainStyles => "シンプル表示",
        Text::ShowFooter => "フッター表示",
        Text::DimFooter => "フッターを暗く",
        Text::FooterAlign => "フッター配置",
        Text::ChapterTitle => "章タイトル",
        Text::ProgressMode => "進捗モード",
        Text::ProgressBar => "進捗バー",
        Text::BarLength => "バーの長さ",
        Text::ProgressPercent => "進捗率",
        Text::ChapterLoc => "章の位置",
        Text::On => "オン",
        Text::Off => "オフ",
        Text::Left => "左",
        Text::Center => "中央",
        Text::Right => "右",
        Text::Chapter => "章",
        Text::Overall => "全体",
        Text::Auto => "自動",
        Text::TerminalTheme => "ターミナル",
        Text::Image => "画像",
        Text::Section => "セクション",
        Text::Usage => "使い方: cargo run -- <EPUBファイルのパス>",
        Text::CannotOpen => "{path} を開けません: {error}",
        Text::NotZip => "{path} は EPUB ではありません（zip アーカイブではありません）",
        Text::NotEpub => "{path} は読み込めない EPUB です（package または spine がありません）",
        Text::NoChapters => "{path} に章が見つかりません",
        Text::TerminalError => "ターミナルエラー: {error}",
    }
}

fn korean(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "목차",
        Text::Settings => "설정",
        Text::MainUi => "기본 화면",
        Text::Footer => "바닥글",
        Text::MaxWidth => "최대 너비",
        Text::MarginLeft => "왼쪽 여백",
        Text::MarginRight => "오른쪽 여백",
        Text::ScrollLines => "스크롤 줄 수",
        Text::Theme => "테마",
        Text::Language => "언어",
        Text::Images => "이미지",
        Text::Blocks => "블록",
        Text::PlainStyles => "기본 스타일",
        Text::ShowFooter => "바닥글 표시",
        Text::DimFooter => "바닥글 흐리게",
        Text::FooterAlign => "바닥글 정렬",
        Text::ChapterTitle => "장 제목",
        Text::ProgressMode => "진행 모드",
        Text::ProgressBar => "진행 막대",
        Text::BarLength => "막대 길이",
        Text::ProgressPercent => "진행률",
        Text::ChapterLoc => "장 위치",
        Text::On => "켜기",
        Text::Off => "끄기",
        Text::Left => "왼쪽",
        Text::Center => "가운데",
        Text::Right => "오른쪽",
        Text::Chapter => "장",
        Text::Overall => "전체",
        Text::Auto => "자동",
        Text::TerminalTheme => "터미널",
        Text::Image => "이미지",
        Text::Section => "섹션",
        Text::Usage => "사용법: cargo run -- <EPUB 파일 경로>",
        Text::CannotOpen => "{path}을(를) 열 수 없습니다: {error}",
        Text::NotZip => "{path}은(는) EPUB이 아닙니다 (zip 파일이 아님)",
        Text::NotEpub => "{path}은(는) 읽을 수 없는 EPUB입니다 (package 또는 spine 없음)",
        Text::NoChapters => "{path}에서 장을 찾을 수 없습니다",
        Text::TerminalError => "터미널 오류: {error}",
    }
}

fn russian(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "Оглавление",
        Text::Settings => "Настройки",
        Text::MainUi => "Основное",
        Text::Footer => "Нижняя строка",
        Text::MaxWidth => "Макс. ширина",
        Text::MarginLeft => "Отступ слева",
        Text::MarginRight => "Отступ справа",
        Text::ScrollLines => "Шаг прокрутки",
        Text::Theme => "Тема",
        Text::Language => "Язык",
        Text::Images => "Изображения",
        Text::Blocks => "Блоки",
        Text::PlainStyles => "Простой стиль",
        Text::ShowFooter => "Показывать",
        Text::DimFooter => "Приглушить",
        Text::FooterAlign => "Выравнивание",
        Text::ChapterTitle => "Название главы",
        Text::ProgressMode => "Режим прогресса",
        Text::ProgressBar => "Шкала прогресса",
        Text::BarLength => "Длина шкалы",
        Text::ProgressPercent => "Прогресс в %",
        Text::ChapterLoc => "Номер главы",
        Text::On => "Вкл",
        Text::Off => "Выкл",
        Text::Left => "Слева",
        Text::Center => "По центру",
        Text::Right => "Справа",
        Text::Chapter => "Глава",
        Text::Overall => "Вся книга",
        Text::Auto => "Авто",
        Text::TerminalTheme => "Терминал",
        Text::Image => "Изображение",
        Text::Section => "Раздел",
        Text::Usage => "Использование: cargo run -- <путь_к_epub>",
        Text::CannotOpen => "не удалось открыть {path}: {error}",
        Text::NotZip => "{path} не является EPUB (это не zip-архив)",
        Text::NotEpub => "{path} — нечитаемый EPUB (нет package или spine)",
        Text::NoChapters => "в {path} не найдено глав",
        Text::TerminalError => "ошибка терминала: {error}",
    }
}

fn spanish(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "Índice",
        Text::Settings => "Ajustes",
        Text::MainUi => "Interfaz",
        Text::Footer => "Pie de página",
        Text::MaxWidth => "Ancho máximo",
        Text::MarginLeft => "Margen izq.",
        Text::MarginRight => "Margen der.",
        Text::ScrollLines => "Desplazamiento",
        Text::Theme => "Tema",
        Text::Language => "Idioma",
        Text::Images => "Imágenes",
        Text::Blocks => "Bloques",
        Text::PlainStyles => "Estilo simple",
        Text::ShowFooter => "Mostrar pie",
        Text::DimFooter => "Atenuar pie",
        Text::FooterAlign => "Alinear pie",
        Text::ChapterTitle => "Título capítulo",
        Text::ProgressMode => "Modo progreso",
        Text::ProgressBar => "Barra progreso",
        Text::BarLength => "Largo de barra",
        Text::ProgressPercent => "Progreso %",
        Text::ChapterLoc => "N.º capítulo",
        Text::On => "Sí",
        Text::Off => "No",
        Text::Left => "Izquierda",
        Text::Center => "Centro",
        Text::Right => "Derecha",
        Text::Chapter => "Capítulo",
        Text::Overall => "Total",
        Text::Auto => "Auto",
        Text::TerminalTheme => "Terminal",
        Text::Image => "Imagen",
        Text::Section => "Sección",
        Text::Usage => "Uso: cargo run -- <ruta_al_epub>",
        Text::CannotOpen => "no se puede abrir {path}: {error}",
        Text::NotZip => "{path} no es un EPUB (no es un archivo zip)",
        Text::NotEpub => "{path} no es un EPUB legible (sin package ni spine)",
        Text::NoChapters => "no se encontraron capítulos en {path}",
        Text::TerminalError => "error de terminal: {error}",
    }
}

fn french(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "Table des matières",
        Text::Settings => "Réglages",
        Text::MainUi => "Interface",
        Text::Footer => "Pied de page",
        Text::MaxWidth => "Largeur max",
        Text::MarginLeft => "Marge gauche",
        Text::MarginRight => "Marge droite",
        Text::ScrollLines => "Défilement",
        Text::Theme => "Thème",
        Text::Language => "Langue",
        Text::Images => "Images",
        Text::Blocks => "Blocs",
        Text::PlainStyles => "Style simple",
        Text::ShowFooter => "Afficher pied",
        Text::DimFooter => "Pied atténué",
        Text::FooterAlign => "Alignement",
        Text::ChapterTitle => "Titre chapitre",
        Text::ProgressMode => "Mode progrès",
        Text::ProgressBar => "Barre progrès",
        Text::BarLength => "Long. barre",
        Text::ProgressPercent => "Progrès %",
        Text::ChapterLoc => "N° chapitre",
        Text::On => "Oui",
        Text::Off => "Non",
        Text::Left => "Gauche",
        Text::Center => "Centré",
        Text::Right => "Droite",
        Text::Chapter => "Chapitre",
        Text::Overall => "Livre",
        Text::Auto => "Auto",
        Text::TerminalTheme => "Terminal",
        Text::Image => "Image",
        Text::Section => "Section",
        Text::Usage => "Utilisation : cargo run -- <chemin_du_epub>",
        Text::CannotOpen => "impossible d’ouvrir {path} : {error}",
        Text::NotZip => "{path} n’est pas un EPUB (pas une archive zip)",
        Text::NotEpub => "{path} n’est pas un EPUB lisible (pas de package ni de spine)",
        Text::NoChapters => "aucun chapitre trouvé dans {path}",
        Text::TerminalError => "erreur du terminal : {error}",
    }
}

fn german(text: Text) -> &'static str {
    match text {
        Text::TableOfContents => "Inhaltsverzeichnis",
        Text::Settings => "Einstellungen",
        Text::MainUi => "Oberfläche",
        Text::Footer => "Fußzeile",
        Text::MaxWidth => "Max. Breite",
        Text::MarginLeft => "Rand links",
        Text::MarginRight => "Rand rechts",
        Text::ScrollLines => "Scrollzeilen",
        Text::Theme => "Farbschema",
        Text::Language => "Sprache",
        Text::Images => "Bilder",
        Text::Blocks => "Blöcke",
        Text::PlainStyles => "Einfacher Stil",
        Text::ShowFooter => "Fußzeile zeigen",
        Text::DimFooter => "Fußzeile dimmen",
        Text::FooterAlign => "Ausrichtung",
        Text::ChapterTitle => "Kapiteltitel",
        Text::ProgressMode => "Fortschrittsart",
        Text::ProgressBar => "Balken zeigen",
        Text::BarLength => "Balkenlänge",
        Text::ProgressPercent => "Fortschritt %",
        Text::ChapterLoc => "Kapitelnummer",
        Text::On => "An",
        Text::Off => "Aus",
        Text::Left => "Links",
        Text::Center => "Mitte",
        Text::Right => "Rechts",
        Text::Chapter => "Kapitel",
        Text::Overall => "Buch",
        Text::Auto => "Auto",
        Text::TerminalTheme => "Terminal",
        Text::Image => "Bild",
        Text::Section => "Abschnitt",
        Text::Usage => "Aufruf: cargo run -- <Pfad_zur_EPUB>",
        Text::CannotOpen => "{path} kann nicht geöffnet werden: {error}",
        Text::NotZip => "{path} ist kein EPUB (kein Zip-Archiv)",
        Text::NotEpub => "{path} ist kein lesbares EPUB (package oder spine fehlt)",
        Text::NoChapters => "keine Kapitel in {path} gefunden",
        Text::TerminalError => "Terminalfehler: {error}",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::width::width;

    #[test]
    fn locales_map_to_languages() {
        assert_eq!(parse_locale("zh_CN.UTF-8"), SimplifiedChinese);
        assert_eq!(parse_locale("zh_TW.UTF-8"), TraditionalChinese);
        assert_eq!(parse_locale("zh-Hant"), TraditionalChinese);
        assert_eq!(parse_locale("zh_Hans_CN"), SimplifiedChinese);
        assert_eq!(parse_locale("ja_JP.UTF-8"), Japanese);
        assert_eq!(parse_locale("de_DE@euro"), German);
        assert_eq!(parse_locale("C.UTF-8"), English);
        assert_eq!(parse_locale(""), English);
    }

    #[test]
    fn every_translation_fits_its_box() {
        let labels = [
            Text::MaxWidth,
            Text::MarginLeft,
            Text::MarginRight,
            Text::ScrollLines,
            Text::Theme,
            Text::Language,
            Text::Images,
            Text::PlainStyles,
            Text::ShowFooter,
            Text::DimFooter,
            Text::FooterAlign,
            Text::ChapterTitle,
            Text::ProgressMode,
            Text::ProgressBar,
            Text::BarLength,
            Text::ProgressPercent,
            Text::ChapterLoc,
        ];
        let values = [
            Text::On,
            Text::Off,
            Text::Left,
            Text::Center,
            Text::Right,
            Text::Chapter,
            Text::Overall,
            Text::Auto,
            Text::TerminalTheme,
            Text::Blocks,
        ];
        for language in ALL {
            let lookup = |text| match language {
                Auto | English => english(text),
                SimplifiedChinese => simplified_chinese(text),
                TraditionalChinese => traditional_chinese(text),
                Japanese => japanese(text),
                Korean => korean(text),
                Russian => russian(text),
                Spanish => spanish(text),
                French => french(text),
                German => german(text),
            };
            // Settings rows give labels 15 columns and values 10
            for label in labels {
                assert!(
                    width(lookup(label)) <= 15,
                    "{:?}: {}",
                    language,
                    lookup(label)
                );
            }
            for value in values {
                assert!(
                    width(lookup(value)) <= 10,
                    "{:?}: {}",
                    language,
                    lookup(value)
                );
            }
            assert!(width(language.name()) <= 10);
            // Box titles and section headers must fit the narrowest boxes
            assert!(width(lookup(Text::TableOfContents)) + 2 <= 28);
            assert!(width(lookup(Text::Settings)) + 2 <= 34);
            for header in [Text::MainUi, Text::Footer] {
                assert!(width(lookup(header)) + 8 <= 34);
            }
        }
    }
}
