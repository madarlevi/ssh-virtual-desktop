import {readFileSync,readdirSync} from 'node:fs';
import {parseDocument} from 'yaml';
for(const path of ['.github/dependabot.yml',...readdirSync('.github/ISSUE_TEMPLATE').filter(f=>/\.ya?ml$/.test(f)).map(f=>`.github/ISSUE_TEMPLATE/${f}`)]){
 const doc=parseDocument(readFileSync(path,'utf8'),{uniqueKeys:true});if(doc.errors.length)throw new Error(`${path}: ${doc.errors.join('; ')}`);console.log(`Valid YAML: ${path}`);
}
for(const file of readdirSync('.github/workflows').filter(f=>/\.ya?ml$/.test(f))){
 const doc=parseDocument(readFileSync(`.github/workflows/${file}`,'utf8'),{uniqueKeys:true});
 if(doc.errors.length)throw new Error(`${file}: ${doc.errors.join('; ')}`);
 const value=doc.toJS();if(!value.on||!value.jobs||!value.permissions)throw new Error(`${file}: missing events, jobs or explicit permissions`);
 for(const job of Object.values(value.jobs))for(const step of job.steps||[]){if(step.uses&&!/^\S+@[a-f0-9]{40}$/.test(step.uses))throw new Error(`${file}: action must use a full commit SHA: ${step.uses}`);}
 console.log(`Valid YAML and pinned actions: ${file}`);
}
