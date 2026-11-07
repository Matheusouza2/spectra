import { Component } from 'react';
import './ErrorBoundary.css';
import { logEvent } from '../services/app-logger';
import { I18nConsumer } from '../i18n/I18nContext';

export default class ErrorBoundary extends Component {
  constructor(props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error) {
    return { hasError: true, error };
  }

  componentDidCatch(error, info) {
    console.error('[ErrorBoundary]', error, info);
    logEvent('ERROR', 'react', 'component.crashed', `${error?.message ?? error}\n${info?.componentStack ?? ''}`);
  }

  render() {
    if (this.state.hasError) {
      return (
        <I18nConsumer>
          {(t) => (
            <div className="error-boundary">
              <div className="error-boundary-icon">⚠</div>
              <h3>{t('common.errorTitle')}</h3>
              <p className="error-boundary-msg">{this.state.error?.message || t('common.unknownError')}</p>
              <button className="error-boundary-btn" onClick={() => this.setState({ hasError: false, error: null })}>
                {t('common.tryAgain')}
              </button>
            </div>
          )}
        </I18nConsumer>
      );
    }
    return this.props.children;
  }
}
