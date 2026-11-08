import React, { useEffect, useMemo, useRef, useState } from 'react';
import { getAlgorithmCatalog } from '../services/tauri-api';
import { useI18n } from '../i18n/I18nContext';
import { SECTIONS, CLASSIC_REFS } from '../data/methodology';
import './MethodologyView.css';

const STATUS_KEY = {
  stable: 'algo.status.stable',
  experimental: 'algo.status.experimental',
  legacy_experimental: 'algo.status.legacy',
  proposed: 'algo.status.proposed',
  under_study: 'algo.status.underStudy',
};

const jump = (id) => {
  const el = document.getElementById(id);
  if (!el) return;
  el.scrollIntoView({ block: 'start', behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
  el.focus({ preventScroll: true });
};

function RefChips({ ids }) {
  const { t } = useI18n();
  if (!ids.length) return <span className="mv-muted">{t('method.noSource')}</span>;
  return ids.map((id) => (
    <button key={id} type="button" className="mv-ref" onClick={() => jump(`mv-ref-${id}`)}>{id}</button>
  ));
}

function Item({ section, item }) {
  const { t } = useI18n();
  const base = `method.sections.${section.id}.items.${item.id}`;
  return (
    <article className="mv-item" id={`mv-${section.id}-${item.id}`} tabIndex={-1}>
      <h4>{t(`${base}.name`)}</h4>
      <dl className="mv-prose">
        <dt>{t('method.what')}</dt><dd>{t(`${base}.what`)}</dd>
        <dt>{t('method.how')}</dt><dd>{t(`${base}.how`)}</dd>
        <dt>{t('method.limits')}</dt><dd>{t(`${base}.limits`)}</dd>
      </dl>
      {item.formulas.length > 0 && (
        <figure className="mv-formulas">
          <figcaption>{t('method.formulas')}</figcaption>
          <pre>{item.formulas.join('\n')}</pre>
        </figure>
      )}
      <footer className="mv-trace">
        <span><span className="mv-muted">{t('method.implementation')}</span> <code>{item.crate}</code></span>
        <span className="mv-sources"><span className="mv-muted">{t('method.sources')}</span> <RefChips ids={item.refs} /></span>
      </footer>
    </article>
  );
}

function CatalogEntry({ entry }) {
  const { t, tm } = useI18n();
  const list = (items) => (items?.length ? <ul>{items.map((x) => <li key={x}>{tm(x)}</li>)}</ul> : <span className="mv-muted">—</span>);
  return (
    <article className="mv-item" id={`mv-catalog-${entry.id}`} tabIndex={-1}>
      <h4>
        {tm(entry.name)}
        <span className={`alg-badge alg-badge-${entry.status}`}>{STATUS_KEY[entry.status] ? t(STATUS_KEY[entry.status]) : entry.status}</span>
      </h4>
      <p className="mv-lede">{tm(entry.description)}</p>
      <dl className="mv-prose">
        <dt>{t('method.live.when')}</dt><dd>{tm(entry.when_to_use) || '—'}</dd>
        <dt>{t('method.live.whenNot')}</dt><dd>{tm(entry.when_not_to_use) || '—'}</dd>
        <dt>{t('method.live.output')}</dt><dd>{tm(entry.output) || '—'}</dd>
        <dt>{t('method.live.preserves')}</dt><dd>{list(entry.preserves)}</dd>
        <dt>{t('method.live.doesNotPreserve')}</dt><dd>{list(entry.does_not_preserve)}</dd>
        {entry.notes && <><dt>{t('method.live.notes')}</dt><dd>{tm(entry.notes)}</dd></>}
      </dl>
      <footer className="mv-trace">
        <span><span className="mv-muted">{t('method.live.role')}</span> {t(`method.live.role.${entry.role}`)}</span>
        <span><span className="mv-muted">{t('method.live.objectives')}</span> <code>{entry.objectives.join(', ')}</code></span>
        <span><span className="mv-muted">{t('method.live.netClasses')}</span> <code>{entry.supported_net_classes.join(', ')}</code></span>
        <span><span className="mv-muted">{t('method.live.version')}</span> <code>{entry.implementation_version || t('method.live.notImplemented')}</code></span>
        <span><span className="mv-muted">{t('method.live.proof')}</span> <code>{entry.proof_mapping || t('method.live.none')}</code></span>
        <span className="mv-sources"><span className="mv-muted">{t('method.sources')}</span> <RefChips ids={entry.reference_ids || []} /></span>
      </footer>
    </article>
  );
}

export default function MethodologyView({ onClose }) {
  const { t, tm } = useI18n();
  const [catalog, setCatalog] = useState(null);
  const [catalogError, setCatalogError] = useState(null);
  const [query, setQuery] = useState('');
  const [activeId, setActiveId] = useState(null);
  const tocRef = useRef(null);
  const titleRef = useRef(null);

  useEffect(() => {
    titleRef.current?.focus();
    let active = true;
    getAlgorithmCatalog().then((c) => active && setCatalog(c)).catch((e) => active && setCatalogError(String(e)));
    const onKey = (e) => { if (e.key === 'Escape' && !e.target.closest?.('input')) onClose(); };
    document.addEventListener('keydown', onKey);
    return () => { active = false; document.removeEventListener('keydown', onKey); };
  }, [onClose]);

  const needle = query.trim().toLowerCase();
  const sections = useMemo(() => SECTIONS.map((section) => ({
    ...section,
    items: section.items.filter((item) => {
      if (!needle) return true;
      const base = `method.sections.${section.id}.items.${item.id}`;
      const haystack = [t(`${base}.name`), t(`${base}.what`), t(`${base}.how`), item.crate, ...item.formulas, ...item.refs].join('\n').toLowerCase();
      return haystack.includes(needle);
    }),
  })).filter((section) => section.items.length > 0), [needle, t]);

  const entries = useMemo(() => (catalog?.algorithms || []).filter((a) => !needle
    || [tm(a.name), a.id, tm(a.description)].join('\n').toLowerCase().includes(needle)), [catalog, needle, tm]);

  const refs = catalog?.references || [];
  const nothing = needle && sections.length === 0 && entries.length === 0;

  const toc = [
    ...sections.map((s) => ({ id: `mv-sec-${s.id}`, label: t(`method.sections.${s.id}.title`), count: s.items.length })),
    { id: 'mv-sec-catalog', label: t('method.live.title'), count: entries.length },
    { id: 'mv-sec-bib', label: t('method.bib.title'), count: refs.length + CLASSIC_REFS.length },
  ];

  const tocKey = toc.map((e) => e.id).join('|');
  useEffect(() => {
    // Scroll-spy: the active entry is the last section whose top has passed the reading line.
    const ids = tocKey.split('|');
    let frame = 0;
    const update = () => {
      frame = 0;
      let current = ids[0];
      for (const id of ids) {
        const el = document.getElementById(id);
        if (el && el.getBoundingClientRect().top <= 140) current = id;
      }
      const scroller = tocRef.current?.closest('.content');
      const scrolls = scroller && scroller.scrollHeight > scroller.clientHeight + 1 && getComputedStyle(scroller).overflowY !== 'visible';
      const atEnd = scrolls
        ? scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 4
        : window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 4;
      setActiveId(atEnd ? ids[ids.length - 1] : current);
    };
    const onScroll = () => { if (!frame) frame = requestAnimationFrame(update); };
    const scroller = tocRef.current?.closest('.content');
    scroller?.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    update();
    return () => {
      if (frame) cancelAnimationFrame(frame);
      scroller?.removeEventListener('scroll', onScroll);
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, [tocKey, catalog]);

  useEffect(() => {
    // Keep the active entry visible inside the (possibly scrollable) index.
    tocRef.current?.querySelector('[aria-current="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [activeId]);

  return (
    <div className="mv">
      <nav className="mv-toc" ref={tocRef} aria-label={t('method.contents')}>
        <button type="button" className="mv-back" onClick={onClose}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M10 3 5 8l5 5" /></svg>
          {t('method.back')}
        </button>
        <ol>
          {toc.map((entry) => (
            <li key={entry.id}><button type="button" aria-current={activeId === entry.id ? 'true' : undefined} onClick={() => jump(entry.id)}><span>{entry.label}</span><small>{entry.count}</small></button></li>
          ))}
        </ol>
      </nav>

      <div className="mv-body">
        <header className="mv-head">
          <h2 ref={titleRef} tabIndex={-1}>{t('method.title')}</h2>
          <p>{t('method.lead')}</p>
          <label className="mv-search">
            <span className="mv-sr">{t('method.search')}</span>
            <input type="search" value={query} placeholder={t('method.searchPlaceholder')} onChange={(e) => setQuery(e.target.value)} />
          </label>
        </header>

        {nothing && <p className="mv-empty" role="status">{t('method.noMatch', { query })}</p>}

        {sections.map((section) => (
          <section key={section.id} id={`mv-sec-${section.id}`} className="mv-section" tabIndex={-1} aria-labelledby={`mv-h-${section.id}`}>
            <h3 id={`mv-h-${section.id}`}>{t(`method.sections.${section.id}.title`)}</h3>
            <p className="mv-lede">{t(`method.sections.${section.id}.intro`)}</p>
            {section.items.map((item) => <Item key={item.id} section={section} item={item} />)}
          </section>
        ))}

        {(!needle || entries.length > 0) && (
          <section id="mv-sec-catalog" className="mv-section" tabIndex={-1} aria-labelledby="mv-h-catalog">
            <h3 id="mv-h-catalog">{t('method.live.title')}</h3>
            <p className="mv-lede">{t('method.live.intro', { version: catalog?.catalog_version ?? '—' })}</p>
            {catalogError && <p role="alert" className="mv-empty">{t('method.live.error', { error: tm(catalogError) })}</p>}
            {!catalog && !catalogError && <p className="mv-muted">{t('method.live.loading')}</p>}
            {entries.map((entry) => <CatalogEntry key={entry.id} entry={entry} />)}
          </section>
        )}

        <section id="mv-sec-bib" className="mv-section" tabIndex={-1} aria-labelledby="mv-h-bib">
          <h3 id="mv-h-bib">{t('method.bib.title')}</h3>
          <p className="mv-lede">{t('method.bib.intro')}</p>
          <ul className="mv-bib">
            {[...refs, ...CLASSIC_REFS].map((ref) => (
              <li key={ref.id} id={`mv-ref-${ref.id}`} tabIndex={-1}>
                <span className="mv-bib-id">{ref.id}</span>
                <span>
                  {ref.citation} ({ref.year}).
                  {ref.doi && <> DOI <a href={`https://doi.org/${ref.doi}`} target="_blank" rel="noreferrer">{ref.doi}</a>.</>}
                  {ref.url && <> <a href={ref.url} target="_blank" rel="noreferrer">{ref.url}</a></>}
                  {ref.access && <em className="mv-muted"> {t('method.bib.access')}: {tm(ref.access)}</em>}
                </span>
              </li>
            ))}
          </ul>
        </section>
      </div>
    </div>
  );
}
