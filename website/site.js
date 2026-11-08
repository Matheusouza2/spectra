const copy = {
  en: {
    title: 'SPECTRA — Petri net analysis',
    description: 'SPECTRA is a desktop workbench for Petri nets: structural analysis, invariants, reachability, coverability and CTL model checking.',
    'nav.skip': 'Skip to content', 'nav.label': 'Main', 'nav.features': 'Features', 'nav.workflow': 'How it works', 'nav.downloads': 'Download',
    'hero.title': 'See what your Petri net can do.',
    'hero.lede': 'SPECTRA is a desktop workbench for Petri nets. Check structure and invariants, build the state space, verify CTL properties, and step through the model token by token.',
    'hero.cta': 'Download SPECTRA', 'hero.source': 'View source', 'hero.fine': 'Free and open source · Windows, Linux and macOS',
    'tab.analysis': 'Analysis', 'tab.graphs': 'Graphs', 'tab.invariants': 'Invariants',
    'net.title': 'Mutual exclusion net', 'net.desc': 'Two processes share one lock. Fire a highlighted transition to move tokens.',
    'p.idle': 'Idle', 'p.wait': 'Waiting', 'p.crit': 'Critical', 'p.lock': 'Lock', 't.req': 'request', 't.enter': 'enter', 't.leave': 'leave',
    'r.marking': 'Marking', 'r.order': 'Idle, Waiting, Critical, Lock', 'r.visited': 'Markings visited', 'r.invariants': 'P-invariants', 'r.inv1': 'Idle + Waiting + Critical', 'r.inv2': 'Critical + Lock', 'r.property': 'Property',
    'prop.holds': 'AG (Critical ≤ 1) holds in all {n} reachable markings; no deadlock',
    'net.hint': 'Click a highlighted transition to fire it.', 'net.hintDone': 'Every reachable marking visited. Reset to play again.',
    'net.auto': 'Auto-play', 'net.reset': 'Reset', 'net.fire': 'Fire transition {t}',
    'features.title': 'One model, every question you can ask it.',
    'f.analysis.name': 'Analysis', 'f.analysis.body': 'More than 25 structural properties: boundedness, liveness, conflicts, siphons and traps, with the verdict and the reason for each.',
    'f.invariants.name': 'Invariants', 'f.invariants.body': 'P- and T-invariants from the incidence matrix, so you can prove conservation laws instead of guessing them.',
    'f.graphs.name': 'Graphs', 'f.graphs.body': 'Reachability graph, or a coverability tree for unbounded nets, with ω shown where the marking grows without limit.',
    'f.ctl.body': 'Write properties in a readable formula language and get a verdict with a witness or a counterexample path.',
    'f.game.body': 'Fire enabled transitions one at a time and watch the marking change, exactly like the model above.',
    'f.reduce.name': 'Reductions', 'f.reduce.body': 'Stubborn sets, symmetries and structural reductions keep the state space small enough to finish.',
    'formats': 'Opens <span class="mono">.pnt</span> files from INA and <span class="mono">Mercury</span> scripts.',
    'workflow.title': 'From a file to an answer in three steps.',
    's1.title': 'Open your net', 's1.body': 'Load a <span class="mono">.pnt</span> file or paste a Mercury script. SPECTRA validates the model before anything runs.',
    's2.title': 'Ask a question', 's2.body': 'Pick a structural check, compute invariants, build the state space, or write a CTL formula.',
    's3.title': 'Read the evidence', 's3.body': 'Inspect graphs, markings and paths, and replay any trace in the token game.',
    'dl.title': 'Download SPECTRA', 'dl.windows': 'Windows 10 or later, 64-bit', 'dl.macos': 'Apple Silicon, disk image', 'dl.linux': 'Debian and Ubuntu, or any distro with AppImage',
    'dl.button': 'Download', 'dl.yours': 'Your system',
    'dl.caution': 'The installers are not code-signed yet. Windows SmartScreen may warn you on first run, and on macOS you may need to allow the app under Privacy &amp; Security.',
    'dl.all': 'All files in the latest release',
    'dl.checking': 'Checking the latest release…', 'dl.ready': 'Latest release: {tag}', 'dl.fallback': 'Could not read the release. The buttons open the releases page.',
    'footer.name': 'Reachability And Coverability Net Analyzer', 'footer.source': 'Source code', 'footer.issues': 'Report an issue',
    'language.label': 'Mudar para português', 'language.short': 'PT',
  },
  pt: {
    title: 'SPECTRA — análise de redes de Petri',
    description: 'SPECTRA é uma bancada desktop para redes de Petri: análise estrutural, invariantes, alcançabilidade, cobertura e verificação CTL.',
    'nav.skip': 'Ir para o conteúdo', 'nav.label': 'Principal', 'nav.features': 'Recursos', 'nav.workflow': 'Como funciona', 'nav.downloads': 'Baixar',
    'hero.title': 'Veja o que a sua rede de Petri pode fazer.',
    'hero.lede': 'SPECTRA é uma bancada desktop para redes de Petri. Verifique estrutura e invariantes, construa o espaço de estados, valide propriedades CTL e percorra o modelo token a token.',
    'hero.cta': 'Baixar o SPECTRA', 'hero.source': 'Ver código-fonte', 'hero.fine': 'Gratuito e de código aberto · Windows, Linux e macOS',
    'tab.analysis': 'Análise', 'tab.graphs': 'Grafos', 'tab.invariants': 'Invariantes',
    'net.title': 'Rede de exclusão mútua', 'net.desc': 'Dois processos compartilham uma trava. Dispare uma transição destacada para mover os tokens.',
    'p.idle': 'Ocioso', 'p.wait': 'Aguardando', 'p.crit': 'Crítica', 'p.lock': 'Trava', 't.req': 'pedir', 't.enter': 'entrar', 't.leave': 'sair',
    'r.marking': 'Marcação', 'r.order': 'Ocioso, Aguardando, Crítica, Trava', 'r.visited': 'Marcações visitadas', 'r.invariants': 'P-invariantes', 'r.inv1': 'Ocioso + Aguardando + Crítica', 'r.inv2': 'Crítica + Trava', 'r.property': 'Propriedade',
    'prop.holds': 'AG (Crítica ≤ 1) vale nas {n} marcações alcançáveis; sem deadlock',
    'net.hint': 'Clique em uma transição destacada para dispará-la.', 'net.hintDone': 'Todas as marcações alcançáveis foram visitadas. Reinicie para jogar de novo.',
    'net.auto': 'Automático', 'net.reset': 'Reiniciar', 'net.fire': 'Disparar a transição {t}',
    'features.title': 'Um modelo, todas as perguntas que você pode fazer a ele.',
    'f.analysis.name': 'Análise', 'f.analysis.body': 'Mais de 25 propriedades estruturais: limitação, vivacidade, conflitos, sifões e armadilhas, com o veredito e o motivo de cada uma.',
    'f.invariants.name': 'Invariantes', 'f.invariants.body': 'P- e T-invariantes a partir da matriz de incidência, para provar leis de conservação em vez de supô-las.',
    'f.graphs.name': 'Grafos', 'f.graphs.body': 'Grafo de alcançabilidade ou árvore de cobertura para redes ilimitadas, com ω onde a marcação cresce sem limite.',
    'f.ctl.body': 'Escreva propriedades em uma linguagem de fórmulas legível e receba o veredito com testemunho ou contraexemplo.',
    'f.game.body': 'Dispare as transições habilitadas uma a uma e veja a marcação mudar, como no modelo acima.',
    'f.reduce.name': 'Reduções', 'f.reduce.body': 'Conjuntos teimosos, simetrias e reduções estruturais mantêm o espaço de estados pequeno o bastante para terminar.',
    'formats': 'Abre arquivos <span class="mono">.pnt</span> do INA e scripts <span class="mono">Mercury</span>.',
    'workflow.title': 'De um arquivo a uma resposta em três passos.',
    's1.title': 'Abra sua rede', 's1.body': 'Carregue um arquivo <span class="mono">.pnt</span> ou cole um script Mercury. O SPECTRA valida o modelo antes de qualquer análise.',
    's2.title': 'Faça uma pergunta', 's2.body': 'Escolha uma verificação estrutural, calcule invariantes, construa o espaço de estados ou escreva uma fórmula CTL.',
    's3.title': 'Leia a evidência', 's3.body': 'Inspecione grafos, marcações e caminhos, e reproduza qualquer traço no jogo de tokens.',
    'dl.title': 'Baixar o SPECTRA', 'dl.windows': 'Windows 10 ou superior, 64 bits', 'dl.macos': 'Apple Silicon, imagem de disco', 'dl.linux': 'Debian e Ubuntu, ou qualquer distro com AppImage',
    'dl.button': 'Baixar', 'dl.yours': 'Seu sistema',
    'dl.caution': 'Os instaladores ainda não são assinados. O SmartScreen do Windows pode alertar na primeira execução, e no macOS pode ser preciso liberar o app em Privacidade e Segurança.',
    'dl.all': 'Todos os arquivos da última versão',
    'dl.checking': 'Verificando a última versão…', 'dl.ready': 'Última versão: {tag}', 'dl.fallback': 'Não foi possível ler a versão. Os botões abrem a página de releases.',
    'footer.name': 'Reachability And Coverability Net Analyzer', 'footer.source': 'Código-fonte', 'footer.issues': 'Relatar um problema',
    'language.label': 'Switch to English', 'language.short': 'EN',
  },
};

const stored = (() => { try { return localStorage.getItem('racon-site-language'); } catch { return null; } })();
let language = stored === 'pt' || stored === 'en' ? stored : (navigator.language || '').toLowerCase().startsWith('pt') ? 'pt' : 'en';
const t = (key, vars = {}) => (copy[language][key] ?? key).replace(/\{(\w+)\}/g, (_, k) => vars[k]);

const languageButton = document.querySelector('#language-toggle');
const downloadStatus = document.querySelector('#download-status');
let releaseTag = null;
let statusState = 'checking';

function renderStatus() {
  downloadStatus.dataset.state = statusState;
  downloadStatus.textContent = statusState === 'ready' ? t('dl.ready', { tag: releaseTag }) : t(statusState === 'fallback' ? 'dl.fallback' : 'dl.checking');
}

function applyLanguage(next) {
  language = next;
  const strings = copy[language];
  document.documentElement.lang = language === 'pt' ? 'pt-BR' : 'en';
  document.title = strings.title;
  document.querySelector('meta[name="description"]').content = strings.description;
  document.querySelectorAll('[data-i18n]').forEach((el) => { if (strings[el.dataset.i18n] !== undefined) el.innerHTML = strings[el.dataset.i18n]; });
  document.querySelectorAll('[data-i18n-aria]').forEach((el) => el.setAttribute('aria-label', strings[el.dataset.i18nAria]));
  languageButton.textContent = strings['language.short'];
  languageButton.setAttribute('aria-label', strings['language.label']);
  try { localStorage.setItem('racon-site-language', language); } catch { /* storage unavailable */ }
  renderStatus();
  game.render();
}

/* ── Token game: mutual exclusion with a shared lock ───────────── */
const PLACES = ['idle', 'wait', 'crit', 'lock'];
const TRANS = {
  req: { pre: [1, 0, 0, 0], post: [0, 1, 0, 0] },
  enter: { pre: [0, 1, 0, 1], post: [0, 0, 1, 0] },
  leave: { pre: [0, 0, 1, 0], post: [1, 0, 0, 1] },
};
const M0 = [2, 0, 0, 1];
const enabled = (m, tr) => TRANS[tr].pre.every((n, i) => m[i] >= n);
const fire = (m, tr) => m.map((v, i) => v - TRANS[tr].pre[i] + TRANS[tr].post[i]);

// Reachability set, computed once so the readout is true for this net.
const reachable = (() => {
  const seen = new Map([[M0.join(), M0]]);
  const queue = [M0];
  while (queue.length) {
    const m = queue.shift();
    Object.keys(TRANS).filter((tr) => enabled(m, tr)).forEach((tr) => {
      const next = fire(m, tr);
      if (!seen.has(next.join())) { seen.set(next.join(), next); queue.push(next); }
    });
  }
  return [...seen.values()];
})();
const mutexHolds = reachable.every((m) => m[2] <= 1);
const noDeadlock = reachable.every((m) => Object.keys(TRANS).some((tr) => enabled(m, tr)));

const game = (() => {
  const $ = (s) => document.querySelector(s);
  let marking = [...M0];
  let visited = new Set([M0.join()]);
  let timer = null;
  const dotPositions = { 1: [[0, 0]], 2: [[-8, 0], [8, 0]], 3: [[-9, 6], [9, 6], [0, -9]] };
  const ns = 'http://www.w3.org/2000/svg';

  function drawTokens(place, count, pop) {
    const g = document.querySelector(`[data-place="${place}"] .tokens`);
    g.replaceChildren();
    if (count >= 1 && count <= 3) {
      dotPositions[count].forEach(([x, y]) => {
        const c = document.createElementNS(ns, 'circle');
        c.setAttribute('cx', x); c.setAttribute('cy', y); c.setAttribute('r', 5.5);
        g.append(c);
      });
    } else if (count > 3) {
      const txt = document.createElementNS(ns, 'text');
      txt.textContent = count;
      g.append(txt);
    }
    if (pop) { g.classList.remove('pop'); void g.getBoundingClientRect(); g.classList.add('pop'); }
  }

  function render(changed = []) {
    PLACES.forEach((p, i) => drawTokens(p, marking[i], changed.includes(i)));
    Object.keys(TRANS).forEach((tr) => {
      const el = document.querySelector(`[data-trans="${tr}"]`);
      const on = enabled(marking, tr);
      el.classList.toggle('enabled', on);
      el.setAttribute('aria-disabled', String(!on));
      el.setAttribute('tabindex', on ? '0' : '-1');
      el.setAttribute('aria-label', t('net.fire', { t: t(`t.${tr}`) }));
    });
    $('#r-marking').textContent = `(${marking.join(', ')})`;
    $('#r-visited').textContent = `${visited.size} / ${reachable.length}`;
    $('#r-inv1').textContent = marking[0] + marking[1] + marking[2];
    $('#r-inv2').textContent = marking[2] + marking[3];
    $('#r-prop').textContent = mutexHolds && noDeadlock ? t('prop.holds', { n: reachable.length }) : '';
    $('#hint').textContent = visited.size === reachable.length ? t('net.hintDone') : t('net.hint');
  }

  function step(tr) {
    if (!enabled(marking, tr)) return;
    const prev = marking;
    marking = fire(marking, tr);
    visited.add(marking.join());
    const el = document.querySelector(`[data-trans="${tr}"]`);
    el.classList.remove('fired'); void el.getBoundingClientRect(); el.classList.add('fired');
    render(marking.map((v, i) => (v !== prev[i] ? i : -1)).filter((i) => i >= 0));
  }

  function stopAuto() {
    clearInterval(timer); timer = null;
    $('#auto').setAttribute('aria-pressed', 'false');
  }

  function init() {
    document.querySelectorAll('[data-trans]').forEach((el) => {
      el.addEventListener('click', () => step(el.dataset.trans));
      el.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); step(el.dataset.trans); }
      });
    });
    $('#reset').addEventListener('click', () => { stopAuto(); marking = [...M0]; visited = new Set([M0.join()]); render(); });
    $('#auto').addEventListener('click', () => {
      if (timer) return stopAuto();
      $('#auto').setAttribute('aria-pressed', 'true');
      timer = setInterval(() => {
        const options = Object.keys(TRANS).filter((tr) => enabled(marking, tr));
        step(options[Math.floor(Math.random() * options.length)]);
      }, 900);
    });
  }

  return { init, render: () => render() };
})();

/* ── Downloads: latest release + detected system ───────────────── */
const repository = 'Matheusouza2/spectra';
const releaseUrl = `https://github.com/${repository}/releases/latest`;
const platformPatterns = { windows: [/\.msi$/i, /\.exe$/i], linux: [/\.deb$/i, /\.appimage$/i], macos: [/\.dmg$/i] };

const ua = navigator.userAgent.toLowerCase();
const mine = ua.includes('win') ? 'windows' : ua.includes('mac') ? 'macos' : ua.includes('linux') && !ua.includes('android') ? 'linux' : null;
if (mine) {
  const row = document.querySelector(`[data-platform="${mine}"]`);
  row.classList.add('mine');
  row.querySelector('.badge').hidden = false;
}

fetch(`https://api.github.com/repos/${repository}/releases/latest`, { headers: { Accept: 'application/vnd.github+json' } })
  .then((r) => { if (!r.ok) throw new Error('release unavailable'); return r.json(); })
  .then((release) => {
    releaseTag = release.tag_name;
    document.querySelectorAll('[data-platform]').forEach((row) => {
      const link = row.querySelector('.button');
      const asset = platformPatterns[row.dataset.platform].map((re) => (release.assets ?? []).find((a) => re.test(a.name))).find(Boolean);
      if (asset) {
        link.href = asset.browser_download_url;
        row.querySelector('.file-kind').textContent = asset.name.slice(asset.name.lastIndexOf('.')).toLowerCase();
      } else {
        link.href = release.html_url || releaseUrl;
      }
    });
    statusState = 'ready'; renderStatus();
  })
  .catch(() => { statusState = 'fallback'; renderStatus(); });

languageButton.addEventListener('click', () => applyLanguage(language === 'en' ? 'pt' : 'en'));
game.init();
applyLanguage(language);
