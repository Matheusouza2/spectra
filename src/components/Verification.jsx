import React, { useState } from 'react';
import './Verification.css';
import CTLEditor from './CTLEditor';
import MarkingPath from './MarkingPath';
import PlaceBoundedness from './PlaceBoundedness';
import DeadTransition from './DeadTransition';
import Distances from './Distances';
import Circuits from './Circuits';
import Predicates from './Predicates';
import BoundedGate from './BoundedGate';
import { useI18n } from '../i18n/I18nContext';

const MODES = ['ctl', 'marking', 'placeBound', 'deadTransition', 'distances', 'circuits', 'predicates'];

/** Queries on the exact reachability graph: a CTL formula, or the way to reach a marking. */
export default function Verification({ filePath, gate, ctlResult, onCtlResult, placeNames, transitionNames, extended = false }) {
  const { t } = useI18n();
  const [mode, setMode] = useState('ctl');
  const [inject, setInject] = useState(null);
  const useInCtl = (text) => { setInject({ text, nonce: Date.now() }); setMode('ctl'); };
  const blocked = gate ? !gate.allowed : false;

  return (
    <div className="vf">
      <header className="vf-head">
        <div>
          <h2>{t('verify.title')}</h2>
          <p>{t('verify.subtitle')}</p>
        </div>
        <div className="vf-switch" role="tablist" aria-label={t('verify.title')}>
          {MODES.map((m) => (
            <button
              key={m}
              type="button"
              role="tab"
              id={`vf-tab-${m}`}
              aria-selected={mode === m}
              aria-controls={`vf-panel-${m}`}
              className={mode === m ? 'on' : ''}
              onClick={() => setMode(m)}
            >
              {t(`verify.mode.${m}`)}
            </button>
          ))}
        </div>
      </header>

      {mode !== 'placeBound' && mode !== 'deadTransition' && <BoundedGate gate={gate} />}

      {/* Both stay mounted so a half-written formula or marking survives switching. */}
      <div role="tabpanel" id="vf-panel-ctl" aria-labelledby="vf-tab-ctl" hidden={mode !== 'ctl'}>
        <CTLEditor embedded filePath={filePath} onResult={onCtlResult} result={ctlResult} gate={gate} placeNames={placeNames} transitionNames={transitionNames} inject={inject} />
      </div>
      <div role="tabpanel" id="vf-panel-marking" aria-labelledby="vf-tab-marking" hidden={mode !== 'marking'}>
        <MarkingPath filePath={filePath} blocked={blocked} extended={extended} />
      </div>
      <div role="tabpanel" id="vf-panel-placeBound" aria-labelledby="vf-tab-placeBound" hidden={mode !== 'placeBound'}>
        <PlaceBoundedness filePath={filePath} extended={extended} />
      </div>
      <div role="tabpanel" id="vf-panel-deadTransition" aria-labelledby="vf-tab-deadTransition" hidden={mode !== 'deadTransition'}>
        <DeadTransition filePath={filePath} extended={extended} />
      </div>
      <div role="tabpanel" id="vf-panel-distances" aria-labelledby="vf-tab-distances" hidden={mode !== 'distances'}>
        <Distances filePath={filePath} blocked={blocked} />
      </div>
      <div role="tabpanel" id="vf-panel-circuits" aria-labelledby="vf-tab-circuits" hidden={mode !== 'circuits'}>
        <Circuits filePath={filePath} blocked={blocked} />
      </div>
      <div role="tabpanel" id="vf-panel-predicates" aria-labelledby="vf-tab-predicates" hidden={mode !== 'predicates'}>
        <Predicates filePath={filePath} blocked={blocked} onUseInCtl={useInCtl} />
      </div>
    </div>
  );
}
