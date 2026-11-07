const copy = {
  en: {
    title: 'RACoN — Petri net analysis, made visible',
    description: 'RACoN is a desktop Petri net analyzer for structural analysis, reachability, coverability, and CTL verification.',
    'nav.label': 'Main navigation', 'nav.about': 'About', 'nav.workflow': 'How it works', 'nav.downloads': 'Downloads', 'nav.cta': 'Get RACoN',
    'hero.eyebrow': 'A desktop workbench for Petri nets', 'hero.title': 'See how your<br /><em>systems can move.</em>', 'hero.lede': 'RACoN turns Petri net models into something you can inspect, explore, and verify — from the first token to the full state space.', 'hero.cta': 'Download RACoN', 'hero.secondary': 'See how to use it', 'hero.meta': 'Free · Desktop · Windows, Linux &amp; macOS', 'hero.diagramAlt': 'Illustration of a Petri net with places, transitions, and token flow',
    'diagram.title': 'PETRI NET / LIVE MODEL', 'diagram.status': 'READY', 'diagram.tag': 'INITIAL MARKING  M₀', 'diagram.places': 'Places', 'diagram.transitions': 'Transitions', 'diagram.tokens': 'Tokens', 'diagram.caption': 'A model is the beginning of the story.',
    'ticker.one': 'STRUCTURAL PROPERTIES', 'ticker.two': 'STATE SPACE', 'ticker.three': 'CTL MODEL CHECKING', 'ticker.four': 'TOKEN GAME', 'ticker.five': 'COVERABILITY',
    'about.kicker': 'A model you can question', 'about.title': 'From formal model<br />to <em>clear answers.</em>', 'about.body': 'Petri nets describe systems where events compete for resources, happen in parallel, or wait on one another. RACoN gives students, researchers, and engineers a practical way to reason about those systems: load a model, inspect its structure, explore possible states, and check whether the behavior meets a formal property.', 'about.note': 'Built for discrete-event systems, workflows, protocols, and any process where “what can happen next?” matters.',
    'cap.one.label': 'UNDERSTAND', 'cap.one.title': 'Read the structure', 'cap.one.body': 'Inspect places, transitions, arcs, incidence matrices, structural properties, and invariants.', 'cap.two.label': 'EXPLORE', 'cap.two.title': 'Trace what can happen', 'cap.two.body': 'Build reachability graphs or coverability results, then follow paths and inspect markings.', 'cap.three.label': 'VERIFY', 'cap.three.title': 'Check a property', 'cap.three.body': 'Evaluate CTL formulas and bad-state queries, with witnesses or counterexamples when available.',
    'workflow.kicker': 'A practical workflow', 'workflow.title': 'Bring a model.<br /><em>Follow the behavior.</em>', 'workflow.intro': 'RACoN keeps the path from input to result visible and interactive.', 'workflow.step1.title': 'Load your net', 'workflow.step1.body': 'Open an INA/PNT `.pnt` file or paste a compatible Mercury script. RACoN validates the model before analysis.', 'workflow.step2.title': 'Choose a question', 'workflow.step2.body': 'Start with structural properties, calculate invariants, generate a state space, or define a CTL or bad-state check.', 'workflow.step3.title': 'Explore the result', 'workflow.step3.body': 'Inspect graphs, markings, paths, evidence, and algorithm limits. Use the token game to step through enabled transitions.',
    'download.kicker': 'Choose your platform', 'download.title': 'Take the workbench<br /><em>to your desktop.</em>', 'download.intro': 'The latest builds are created from the `main` branch. Pick your system to download its installer.', 'download.all': 'View all files in the latest release ↗', 'download.windows': 'Windows · 64-bit', 'download.linux': 'Ubuntu / Debian · 64-bit', 'download.macos': 'macOS · Disk image', 'download.button': 'Download', 'download.status': 'Checking the latest release…', 'download.ready': 'Latest installer links are ready.', 'download.fallback': 'Open the latest release to choose a download.', 'download.caution': 'macOS packages are ad-hoc signed, not notarized. macOS may require you to allow the app under Privacy &amp; Security after installation.',
    'closing.line': 'A model is a question. RACoN helps you follow the answer.', 'closing.cta': 'Find your download ↓', 'footer.source': 'Source code', 'footer.issues': 'Report an issue', 'language.label': 'Switch to Portuguese',
  },
  pt: {
    title: 'RACoN — análise de redes de Petri, mais visível',
    description: 'RACoN é um analisador desktop de redes de Petri para análise estrutural, alcançabilidade, cobertura e verificação CTL.',
    'nav.label': 'Navegação principal', 'nav.about': 'Sobre', 'nav.workflow': 'Como funciona', 'nav.downloads': 'Downloads', 'nav.cta': 'Obter RACoN',
    'hero.eyebrow': 'Uma bancada desktop para redes de Petri', 'hero.title': 'Veja como seus<br /><em>sistemas podem evoluir.</em>', 'hero.lede': 'O RACoN transforma modelos de redes de Petri em algo que você pode inspecionar, explorar e verificar — do primeiro token a todo o espaço de estados.', 'hero.cta': 'Baixar RACoN', 'hero.secondary': 'Veja como usar', 'hero.meta': 'Gratuito · Desktop · Windows, Linux e macOS', 'hero.diagramAlt': 'Ilustração de uma rede de Petri com lugares, transições e fluxo de tokens',
    'diagram.title': 'REDE DE PETRI / MODELO', 'diagram.status': 'PRONTA', 'diagram.tag': 'MARCAÇÃO INICIAL  M₀', 'diagram.places': 'Lugares', 'diagram.transitions': 'Transições', 'diagram.tokens': 'Tokens', 'diagram.caption': 'O modelo é o começo da história.',
    'ticker.one': 'PROPRIEDADES ESTRUTURAIS', 'ticker.two': 'ESPAÇO DE ESTADOS', 'ticker.three': 'VERIFICAÇÃO CTL', 'ticker.four': 'JOGO DE TOKENS', 'ticker.five': 'COBERTURA',
    'about.kicker': 'Um modelo que você pode questionar', 'about.title': 'Do modelo formal<br />a <em>respostas claras.</em>', 'about.body': 'Redes de Petri descrevem sistemas em que eventos disputam recursos, ocorrem em paralelo ou aguardam uns pelos outros. O RACoN oferece a estudantes, pesquisadores e engenheiros uma forma prática de analisar esses sistemas: carregar um modelo, inspecionar sua estrutura, explorar estados possíveis e verificar se o comportamento satisfaz uma propriedade formal.', 'about.note': 'Feito para sistemas a eventos discretos, fluxos de trabalho, protocolos e processos em que importa saber “o que pode acontecer agora?”.',
    'cap.one.label': 'ENTENDA', 'cap.one.title': 'Leia a estrutura', 'cap.one.body': 'Inspecione lugares, transições, arcos, matrizes de incidência, propriedades estruturais e invariantes.', 'cap.two.label': 'EXPLORE', 'cap.two.title': 'Percorra os comportamentos', 'cap.two.body': 'Gere grafos de alcançabilidade ou resultados de cobertura; siga caminhos e inspecione marcações.', 'cap.three.label': 'VERIFIQUE', 'cap.three.title': 'Confira propriedades', 'cap.three.body': 'Avalie fórmulas CTL e consultas de estados indesejados, com testemunhos ou contraexemplos quando disponíveis.',
    'workflow.kicker': 'Um fluxo de trabalho prático', 'workflow.title': 'Traga o modelo.<br /><em>Acompanhe o comportamento.</em>', 'workflow.intro': 'No RACoN, o caminho da entrada ao resultado permanece visível e interativo.', 'workflow.step1.title': 'Carregue a rede', 'workflow.step1.body': 'Abra um arquivo `.pnt` no formato INA/PNT ou cole um script Mercury compatível. O RACoN valida o modelo antes da análise.', 'workflow.step2.title': 'Escolha uma pergunta', 'workflow.step2.body': 'Comece pelas propriedades estruturais, calcule invariantes, gere o espaço de estados ou configure uma verificação CTL ou de bad states.', 'workflow.step3.title': 'Explore o resultado', 'workflow.step3.body': 'Inspecione grafos, marcações, caminhos, evidências e limites dos algoritmos. Use o jogo de tokens para disparar transições habilitadas.',
    'download.kicker': 'Escolha sua plataforma', 'download.title': 'Leve a bancada<br /><em>para seu computador.</em>', 'download.intro': 'As versões mais recentes são geradas a partir da branch `main`. Escolha seu sistema para baixar o instalador.', 'download.all': 'Ver todos os arquivos da versão mais recente ↗', 'download.windows': 'Windows · 64 bits', 'download.linux': 'Ubuntu / Debian · 64 bits', 'download.macos': 'macOS · Imagem de disco', 'download.button': 'Baixar', 'download.status': 'Verificando a versão mais recente…', 'download.ready': 'Links dos instaladores atualizados.', 'download.fallback': 'Abra a versão mais recente para escolher um download.', 'download.caution': 'Os pacotes para macOS têm assinatura ad hoc, sem notarização. Depois da instalação, talvez seja necessário autorizar o app em Privacidade e Segurança.',
    'closing.line': 'Um modelo é uma pergunta. O RACoN ajuda você a percorrer a resposta.', 'closing.cta': 'Encontre seu download ↓', 'footer.source': 'Código-fonte', 'footer.issues': 'Relatar um problema', 'language.label': 'Mudar para inglês',
  },
};

const languageButton = document.querySelector('#language-toggle');
let language = localStorage.getItem('racon-site-language') === 'pt' ? 'pt' : 'en';

function applyLanguage(nextLanguage) {
  language = nextLanguage;
  const strings = copy[language];
  document.documentElement.lang = language === 'pt' ? 'pt-BR' : 'en';
  document.title = strings.title;
  document.querySelector('meta[name="description"]').content = strings.description;
  document.querySelectorAll('[data-i18n]').forEach((element) => {
    const value = strings[element.dataset.i18n];
    if (value !== undefined) element.innerHTML = value;
  });
  document.querySelectorAll('[data-i18n-aria]').forEach((element) => {
    const value = strings[element.dataset.i18nAria];
    if (value !== undefined) element.setAttribute('aria-label', value);
  });
  languageButton.innerHTML = language === 'en' ? 'PT <span>⌄</span>' : 'EN <span>⌄</span>';
  languageButton.setAttribute('aria-label', strings['language.label']);
  localStorage.setItem('racon-site-language', language);
}

languageButton.addEventListener('click', () => applyLanguage(language === 'en' ? 'pt' : 'en'));
applyLanguage(language);
document.querySelector('#year').textContent = new Date().getFullYear();

const repository = 'Matheusouza2/spectra';
const releaseUrl = `https://github.com/${repository}/releases/latest`;
const downloadStatus = document.querySelector('#download-status');

fetch(`https://api.github.com/repos/${repository}/releases/latest`, {
  headers: { Accept: 'application/vnd.github+json' },
})
  .then((response) => {
    if (!response.ok) throw new Error('Release metadata unavailable');
    return response.json();
  })
  .then((release) => {
    const assets = release.assets ?? [];
    const patterns = {
      windows: /\.(msi|exe)$/i,
      linux: /\.(deb|appimage)$/i,
      macos: /\.dmg$/i,
    };
    const preferences = { windows: ['.msi', '.exe'], linux: ['.deb', '.appimage'], macos: ['.dmg'] };
    document.querySelectorAll('[data-platform]').forEach((card) => {
      const platform = card.dataset.platform;
      const candidates = assets.filter((asset) => patterns[platform].test(asset.name));
      const asset = candidates.sort((a, b) => preferences[platform].indexOf(a.name.slice(a.name.lastIndexOf('.')).toLowerCase()) - preferences[platform].indexOf(b.name.slice(b.name.lastIndexOf('.')).toLowerCase()))[0];
      const link = card.querySelector('.download-button');
      if (asset) {
        link.href = asset.browser_download_url;
        link.download = asset.name;
        card.querySelector('.file-kind').textContent = asset.name.slice(asset.name.lastIndexOf('.')).toUpperCase();
      } else {
        link.href = release.html_url || releaseUrl;
      }
    });
    downloadStatus.dataset.state = 'ready';
    downloadStatus.textContent = copy[language]['download.ready'];
  })
  .catch(() => {
    document.querySelectorAll('.download-button').forEach((link) => { link.href = releaseUrl; });
    downloadStatus.dataset.state = 'fallback';
    downloadStatus.textContent = copy[language]['download.fallback'];
  });

languageButton.addEventListener('click', () => {
  if (downloadStatus.textContent) {
    const key = downloadStatus.dataset.state === 'ready' ? 'download.ready' : 'download.fallback';
    downloadStatus.textContent = copy[language][key];
  }
});
downloadStatus.dataset.state = 'checking';
