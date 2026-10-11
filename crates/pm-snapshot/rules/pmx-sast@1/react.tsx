export function Unsafe({ html }: { html: string }) {
  // ruleid: pmx-react-dangerous-html
  return <div dangerouslySetInnerHTML={{ __html: html }} />;
}

export function Literal() {
  // ok: pmx-react-dangerous-html
  return <div dangerouslySetInnerHTML={{ __html: "<br/>" }} />;
}

export function Sanitized({ bio }: { bio: string }) {
  // ok: pmx-react-dangerous-html
  return <div className="bio" dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(bio) }} />;
}

export function Text({ html }: { html: string }) {
  // ok: pmx-react-dangerous-html
  return <p>{html}</p>;
}

export function Styles({ css }: { css: string }) {
  // ok: pmx-react-dangerous-html
  return <style dangerouslySetInnerHTML={{ __html: css }} />;
}

export function JsLink({ bio }: { bio: string }) {
  // ruleid: pmx-react-javascript-url
  return <a href={"javascript:" + bio}>x</a>;
}

export function Link({ bio }: { bio: string }) {
  // ok: pmx-react-javascript-url
  return <a href={"/users/" + bio}>x</a>;
}

declare const DOMPurify: { sanitize(s: string): string };
