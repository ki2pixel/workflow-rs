/**
 * DOMDiff - Ultra-lightweight DOM diffing / morphing utility.
 * Morphs a live DOM node to match a target DOM node or HTML string in-place with minimal mutations.
 */
export class DOMDiff {
    /**
     * Morph a live DOM element to match a new structure (HTML string or DOM Node).
     * @param {HTMLElement} realNode - The live DOM element in the document.
     * @param {HTMLElement|string} virtualNodeOrHTML - The target DOM element or HTML string.
     */
    static morph(realNode, virtualNodeOrHTML) {
        if (!realNode) return;

        let virtualNode = virtualNodeOrHTML;
        if (typeof virtualNodeOrHTML === 'string') {
            const template = document.createElement('template');
            template.innerHTML = virtualNodeOrHTML.trim();
            virtualNode = template.content.firstElementChild || template.content.firstChild;
        }

        if (!virtualNode) return;
        DOMDiff._morphNode(realNode, virtualNode);
    }

    /**
     * Recursively morphs a real node into a virtual node.
     * @private
     */
    static _morphNode(real, virtual) {
        // If node types are different, we can't morph, must replace
        if (real.nodeType !== virtual.nodeType) {
            real.replaceWith(virtual.cloneNode(true));
            return;
        }

        // If it's a text node or comment, update text content if changed
        if (real.nodeType === Node.TEXT_NODE || real.nodeType === Node.COMMENT_NODE) {
            if (real.nodeValue !== virtual.nodeValue) {
                real.nodeValue = virtual.nodeValue;
            }
            return;
        }

        // If it's an element node
        if (real.nodeType === Node.ELEMENT_NODE) {
            // 1. If tag names differ, replace the element
            if (real.tagName !== virtual.tagName) {
                real.replaceWith(virtual.cloneNode(true));
                return;
            }

            // 2. Morph attributes
            DOMDiff._morphAttributes(real, virtual);

            // 3. Morph children
            DOMDiff._morphChildren(real, virtual);
        }
    }

    /**
     * Morphs attributes of a real element to match a virtual element.
     * @private
     */
    static _morphAttributes(real, virtual) {
        const realAttrs = real.attributes;
        const virtualAttrs = virtual.attributes;

        // Remove attributes that are not in virtual
        // We iterate backwards because removing attributes changes the length
        for (let i = realAttrs.length - 1; i >= 0; i--) {
            const attrName = realAttrs[i].name;
            if (!virtual.hasAttribute(attrName)) {
                real.removeAttribute(attrName);
            }
        }

        // Add or update attributes from virtual
        for (let i = 0; i < virtualAttrs.length; i++) {
            const attr = virtualAttrs[i];
            if (real.getAttribute(attr.name) !== attr.value) {
                real.setAttribute(attr.name, attr.value);
            }
        }

        // Handle special properties that are not fully mapped by attributes
        const specialProps = ['value', 'checked', 'disabled'];
        for (const prop of specialProps) {
            if (prop in virtual && real[prop] !== virtual[prop]) {
                real[prop] = virtual[prop];
            }
        }
    }

    /**
     * Morphs children of a real element to match children of a virtual element.
     * @private
     */
    static _morphChildren(real, virtual) {
        const realChildren = Array.from(real.childNodes);
        const virtualChildren = Array.from(virtual.childNodes);

        const virtualLen = virtualChildren.length;

        // We use a basic index-based diffing with simple keyed elements heuristic
        // To make it ultra-lightweight and robust, we map nodes by a key attribute
        // to allow elements to be reordered without destroying them.
        const realKeyedMap = new Map();
        const realUnkeyed = [];

        realChildren.forEach((child) => {
            if (child.nodeType === Node.ELEMENT_NODE) {
                const key = child.getAttribute('data-key') || child.id;
                if (key) {
                    realKeyedMap.set(key, child);
                } else {
                    realUnkeyed.push(child);
                }
            } else {
                realUnkeyed.push(child);
            }
        });

        // Loop through the virtual children to align the real children
        for (let i = 0; i < virtualLen; i++) {
            const vChild = virtualChildren[i];
            const vKey = vChild.nodeType === Node.ELEMENT_NODE ? (vChild.getAttribute('data-key') || vChild.id) : null;
            
            let matchedRealNode = null;
            
            if (vKey && realKeyedMap.has(vKey)) {
                matchedRealNode = realKeyedMap.get(vKey);
                // Node matched by key!
            } else if (!vKey && realUnkeyed.length > 0) {
                // Pop the first unkeyed node of similar type (e.g., text vs element)
                const typeMatchIndex = realUnkeyed.findIndex(node => node.nodeType === vChild.nodeType && (node.nodeType !== Node.ELEMENT_NODE || node.tagName === vChild.tagName));
                if (typeMatchIndex !== -1) {
                    matchedRealNode = realUnkeyed.splice(typeMatchIndex, 1)[0];
                }
            }

            if (matchedRealNode) {
                // Morph matched node in place
                DOMDiff._morphNode(matchedRealNode, vChild);
                
                // Ensure the node is in the correct position in the live DOM
                const currentAtPos = real.childNodes[i];
                if (currentAtPos !== matchedRealNode) {
                    real.insertBefore(matchedRealNode, currentAtPos || null);
                }
            } else {
                // Node does not exist in real, let's create it and insert it
                const newNode = vChild.cloneNode(true);
                const currentAtPos = real.childNodes[i];
                real.insertBefore(newNode, currentAtPos || null);
            }
        }

        // Cleanup: remove extra real children that are no longer needed
        const remainingRealChildren = Array.from(real.childNodes).slice(virtualLen);
        remainingRealChildren.forEach(child => child.remove());
    }
}
