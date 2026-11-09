<script setup lang="ts">
import { toRefs } from "vue";
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
import { entryName, groupLabel, kindName, timestamp } from "../utils/display";
import ActionIcon from "./ActionIcon.vue";
import { savedSecret } from "../utils/credentialForm";
const props = defineProps<{ v: UnwrapRef<VaultUI> }>();
const {
  creating,
  editing,
  formKind,
  dirty,
  saving,
  form,
  formError,
  revealed,
  organization,
} = toRefs(props.v);
const {
  save,
  copy,
  copyGroup,
  reveal,
  updateGroupSecret,
  addService,
  addGroup,
  removeGroup,
  askDelete,
  closeForm,
  duplicate,
  openUrl,
} = props.v;
type Secret = "password" | "token";
const secretFields = ["password", "token"] as const;
const secretLabel = { password: "密码", token: "Token" } as const;
const shown = (index: number, field: Secret) =>
  `${index}:${field}` in revealed.value;
const saved = (index: number, field: Secret) => {
  const group = form.value.groups[index];
  return !!group && savedSecret(group, field);
};
const secretPlaceholder = (index: number, field: Secret) =>
  saved(index, field)
    ? "已保存，输入新值替换"
    : editing.value && form.value.groups[index]?.id
      ? "未设置"
      : field === "password"
        ? "输入密码"
        : "输入 API Key / Token";
const databaseTypes = [
  "MySQL",
  "PostgreSQL",
  "Oracle",
  "SQL Server",
  "MariaDB",
  "SQLite",
  "IBM Db2",
  "SAP HANA",
  "SAP ASE",
  "Firebird",
  "Microsoft Access",
  "TiDB",
  "OceanBase",
  "达梦 DM",
  "人大金仓 KingbaseES",
  "openGauss",
  "GaussDB",
  "PolarDB",
  "CockroachDB",
  "YugabyteDB",
  "Redis",
  "Valkey",
  "MongoDB",
  "Amazon DynamoDB",
  "Couchbase",
  "Apache CouchDB",
  "Apache Cassandra",
  "ScyllaDB",
  "Elasticsearch",
  "OpenSearch",
  "Neo4j",
  "ArangoDB",
  "ClickHouse",
  "Apache Doris",
  "StarRocks",
  "DuckDB",
  "InfluxDB",
  "TDengine",
  "TimescaleDB",
  "Snowflake",
  "Google BigQuery",
  "Amazon Redshift",
  "Milvus",
  "Qdrant",
  "Weaviate",
  "Pinecone",
  "其他",
];
function canOpen(value: string) {
  try {
    const url = new URL(value.trim());
    return (
      value.length <= 8192 &&
      !/[\x00-\x1f\x7f]/.test(value) &&
      ["http:", "https:"].includes(url.protocol) &&
      !!url.hostname &&
      !url.username &&
      !url.password
    );
  } catch {
    return false;
  }
}
function addTag(event: Event) {
  const field = event.target as HTMLInputElement;
  const tag = field.value.trim();
  if (tag && !form.value.tags.includes(tag)) {
    form.value.tags.push(tag);
  }
  field.value = "";
}
</script>
<template>
  <section v-if="v.creating || v.editing" class="detail">
    <form id="entry-form" class="inline-editor" @submit.prevent="save">
      <div class="editor-content">
        <div class="detail-title">
          <div class="detail-heading">
            <h2 id="detail-title">
              {{ editing ? entryName(editing) : `新增${kindName(formKind)}` }}
            </h2>
            <span class="tag">{{ kindName(formKind) }}</span>
          </div>
          <span :class="dirty || creating ? 'draft-state' : 'save-state'">{{
            dirty || creating ? "未保存" : "已保存"
          }}</span>
        </div>
        <div v-if="editing" class="detail-toolbar">
          <button
            type="button"
            class="secondary"
            id="duplicate-entry"
            :disabled="saving"
            @click="duplicate()"
          >
            复制为新凭据
          </button>
        </div>
        <fieldset :disabled="saving">
          <div class="identity-fields">
            <div class="form-field">
              <label for="f-kind">凭据类型</label
              ><select id="f-kind" v-model="formKind" :disabled="!!editing">
                <option value="password">账号密码</option>
                <option value="api">API 凭据</option>
                <option value="database">数据库</option>
                <option value="generic">通用</option>
              </select>
            </div>
          </div>
          <div class="form-field">
            <label for="f-tags">标签</label>
            <div class="chips">
              <button
                v-for="tag in form.tags"
                :key="tag"
                type="button"
                class="chip"
                :aria-label="`移除标签 ${tag}`"
                @click="form.tags = form.tags.filter((t) => t !== tag)"
              >
                {{ tag }} ×
              </button>
            </div>
            <input
              id="f-tags"
              list="tag-options"
              placeholder="选择或输入标签，按 Enter 添加"
              @keydown.enter.prevent="addTag"
              @change="addTag"
            /><datalist id="tag-options">
              <option v-for="t in organization.tags" :key="t" :value="t" />
            </datalist>
          </div>
          <div v-if="formKind === 'database'" class="database-fields">
            <div class="form-field">
              <label for="f-db-type">数据库类型</label
              ><input
                id="f-db-type"
                v-model="form.database.databaseType"
                list="database-types"
                required
                placeholder="输入搜索数据库类型"
              /><datalist id="database-types">
                <option v-for="t in databaseTypes" :key="t" :value="t" />
              </datalist>
            </div>
            <div class="host-fields">
              <div class="form-field">
                <label for="f-db-host">主机地址</label>
                <div class="input-actions">
                  <input id="f-db-host" v-model="form.database.host" /><button
                    type="button"
                    class="icon-action"
                    aria-label="复制主机"
                    @click="v.copy('host')"
                  >
                    <ActionIcon name="copy" />
                  </button>
                </div>
              </div>
              <div class="form-field">
                <label for="f-db-port">端口</label
                ><input
                  id="f-db-port"
                  v-model="form.database.port"
                  inputmode="numeric"
                  pattern="[0-9]*"
                  maxlength="5"
                />
              </div>
            </div>
            <div class="form-field">
              <label for="f-db-name">{{
                form.database.databaseType === "Oracle"
                  ? "服务名 / SID"
                  : form.database.databaseType === "Redis"
                    ? "数据库编号"
                    : ["SQLite", "DuckDB", "Microsoft Access"].includes(
                          form.database.databaseType,
                        )
                      ? "数据库文件路径"
                      : "数据库名"
              }}</label>
              <div class="input-actions">
                <input
                  id="f-db-name"
                  v-model="form.database.databaseName"
                /><button
                  type="button"
                  class="icon-action"
                  aria-label="复制数据库名"
                  @click="v.copy('database_name')"
                >
                  <ActionIcon name="copy" />
                </button>
              </div>
            </div>
          </div>
          <div v-if="formKind === 'generic'" class="form-field">
            <label for="f-title"
              >名称<span class="optional"
                >可选，填写网址时按网站显示</span
              ></label
            ><input
              id="f-title"
              v-model="form.title"
              maxlength="256"
              placeholder="例如：家庭 Wi-Fi"
            />
          </div>
          <div
            v-if="formKind === 'password' || formKind === 'generic'"
            class="form-field"
          >
            <label for="f-url"
              >{{ formKind === "password" ? "网站" : "网址"
              }}<span v-if="formKind === 'generic'" class="optional"
                >可选</span
              ></label
            >
            <div class="input-actions">
              <input
                id="f-url"
                v-model="form.url"
                maxlength="4096"
                :required="formKind === 'password'"
                placeholder="https://example.com"
              /><button
                type="button"
                class="icon-action"
                data-open="url"
                :disabled="!canOpen(form.url)"
                aria-label="打开网址"
                title="用默认浏览器打开网址"
                @click="openUrl(form.url)"
              >
                <ActionIcon name="external" /></button
              ><button
                v-if="editing"
                type="button"
                class="icon-action"
                data-copy="url"
                :disabled="!form.url"
                aria-label="复制网址"
                title="复制网址"
                @click="copy('url')"
              >
                <ActionIcon name="copy" />
              </button>
            </div>
          </div>
          <div
            v-if="formKind === 'api' || formKind === 'password'"
            class="form-field api-services-editor"
          >
            <div class="services-editor-heading">
              <label
                >{{ formKind === "api" ? "网站 / API 地址" : "API 地址"
                }}<span v-if="formKind === 'password'" class="optional"
                  >可选，供 Token 调用</span
                ></label
              ><button
                id="add-service"
                class="text-btn"
                type="button"
                :disabled="form.services.length >= 20"
                @click="addService"
              >
                ＋ 添加地址
              </button>
            </div>
            <div
              v-for="(service, index) in form.services"
              :key="index"
              class="service-editor-row"
              :data-service-index="index"
            >
              <div class="service-controls">
                <select
                  :id="`f-protocol-${index}`"
                  v-model="service.protocol"
                  :aria-label="`服务 ${index + 1} 协议`"
                  :title="
                    service.protocol === 'custom'
                      ? '标准 HTTP/HTTPS 接口'
                      : undefined
                  "
                >
                  <option value="openai">OpenAI 兼容</option>
                  <option value="anthropic">Anthropic</option>
                  <option value="gemini">Gemini</option>
                  <option value="custom">HTTP/HTTPS</option>
                </select>
                <input
                  :id="
                    index === 0 && formKind === 'api'
                      ? 'f-url'
                      : `f-url-${index}`
                  "
                  v-model="service.url"
                  maxlength="8192"
                  :aria-label="`服务 ${index + 1} 地址`"
                  :title="service.url"
                  placeholder="服务 Base URL"
                />
                <button
                  type="button"
                  class="icon-action"
                  :data-open-service="index"
                  :disabled="!canOpen(service.url)"
                  :aria-label="`打开服务 ${index + 1} 地址`"
                  title="用默认浏览器打开地址"
                  @click="openUrl(service.url)"
                >
                  <ActionIcon name="external" />
                </button>
                <button
                  type="button"
                  class="icon-action"
                  :data-copy-service="service.id || index"
                  :disabled="!service.url.trim()"
                  :aria-label="`复制服务 ${index + 1} 地址`"
                  title="复制地址"
                  @click="copy('url', service.id, index)"
                >
                  <ActionIcon name="copy" />
                </button>
                <button
                  type="button"
                  class="icon-action remove-action"
                  :data-remove-service="index"
                  :disabled="formKind === 'api' && form.services.length <= 1"
                  :aria-label="`移除服务 ${index + 1}`"
                  title="移除地址"
                  @click="form.services.splice(index, 1)"
                >
                  <ActionIcon name="remove" />
                </button>
              </div>
            </div>
            <small class="service-hint">{{
              formKind === "api"
                ? "至少填写一个地址；各协议地址共用同一 Token，仅保存与填写。"
                : "这个网站的 Token 需要调用接口时填写。"
            }}</small>
          </div>
          <div v-if="formKind === 'api'" class="form-field">
            <label for="f-documentation-url"
              >文档地址<span class="optional">可选</span></label
            >
            <div class="input-actions">
              <input
                id="f-documentation-url"
                v-model="form.documentationUrl"
                maxlength="8192"
                placeholder="https://example.com/docs"
              /><button
                type="button"
                class="icon-action"
                data-open="documentation"
                :disabled="!canOpen(form.documentationUrl)"
                aria-label="打开文档地址"
                title="用默认浏览器打开文档"
                @click="openUrl(form.documentationUrl)"
              >
                <ActionIcon name="external" />
              </button>
            </div>
          </div>
          <div class="form-field groups-editor">
            <div class="services-editor-heading">
              <label
                >凭据<span class="optional">不同环境或账号各占一组</span></label
              ><button
                id="add-group"
                class="text-btn"
                type="button"
                :disabled="form.groups.length >= 50"
                @click="addGroup"
              >
                ＋ 添加一组
              </button>
            </div>
            <div
              v-for="(group, index) in form.groups"
              :key="index"
              class="group-card"
              :data-group-index="index"
            >
              <div class="group-heading">
                <strong>{{ groupLabel(group) || `第 ${index + 1} 组` }}</strong
                ><button
                  type="button"
                  class="icon-action remove-action"
                  :data-remove-group="index"
                  :disabled="form.groups.length <= 1"
                  :aria-label="`移除第 ${index + 1} 组凭据`"
                  title="移除这一组"
                  @click="removeGroup(index)"
                >
                  <ActionIcon name="remove" />
                </button>
              </div>
              <div class="identity-fields">
                <div class="form-field">
                  <label :for="`f-environment-${index}`"
                    >环境<span class="optional">可选</span></label
                  ><input
                    :id="`f-environment-${index}`"
                    v-model="group.environment"
                    maxlength="256"
                    placeholder="例如：生产 / 测试"
                  />
                </div>
                <div class="form-field">
                  <label :for="`f-account-${index}`"
                    >账号<span class="optional">可选</span></label
                  >
                  <div class="input-actions">
                    <input
                      :id="`f-account-${index}`"
                      v-model="group.account"
                      autocomplete="off"
                      maxlength="1024"
                      placeholder="用户名或邮箱"
                    /><button
                      type="button"
                      class="icon-action"
                      :data-copy-account="index"
                      :disabled="!group.account"
                      aria-label="复制账号"
                      title="复制账号"
                      @click="copyGroup(index, 'account')"
                    >
                      <ActionIcon name="copy" />
                    </button>
                  </div>
                </div>
              </div>
              <div
                v-for="field in secretFields"
                :key="field"
                class="form-field"
              >
                <label :for="`f-${field}-${index}`">{{
                  secretLabel[field]
                }}</label>
                <div class="input-actions">
                  <input
                    :id="`f-${field}-${index}`"
                    :value="group[field] || revealed[`${index}:${field}`] || ''"
                    @input="
                      updateGroupSecret(
                        index,
                        field,
                        ($event.target as HTMLInputElement).value,
                      )
                    "
                    :type="shown(index, field) ? 'text' : 'password'"
                    autocomplete="new-password"
                    maxlength="131072"
                    :placeholder="secretPlaceholder(index, field)"
                  /><button
                    type="button"
                    class="icon-action"
                    :data-reveal="`${index}:${field}`"
                    :disabled="!group[field] && !saved(index, field)"
                    :aria-label="`${shown(index, field) ? '隐藏' : '显示'}${secretLabel[field]}`"
                    :title="`${shown(index, field) ? '隐藏' : '显示'}${secretLabel[field]}`"
                    @click="reveal(index, field)"
                  >
                    <ActionIcon
                      :name="shown(index, field) ? 'eye-off' : 'eye'"
                    /></button
                  ><button
                    type="button"
                    class="icon-action"
                    :data-copy-secret="`${index}:${field}`"
                    :disabled="!group[field] && !saved(index, field)"
                    :aria-label="`复制${secretLabel[field]}`"
                    :title="`复制${secretLabel[field]}`"
                    @click="copyGroup(index, field)"
                  >
                    <ActionIcon name="copy" />
                  </button>
                </div>
              </div>
            </div>
            <small class="service-hint"
              >密码和 Token 可只填其一；已保存的值留空即保留。</small
            >
          </div>
          <div class="form-field">
            <label for="f-notes">备注<span class="optional">可选</span></label
            ><textarea
              id="f-notes"
              v-model="form.notes"
              maxlength="32768"
              placeholder="用途或补充说明"
            ></textarea>
          </div>
        </fieldset>
        <p v-if="formError" id="form-error" class="form-error" role="alert">
          {{ formError }}
        </p>
        <div v-if="editing" class="metadata">
          更新于 {{ timestamp(editing.updatedAt) }}
        </div>
      </div>
      <div class="editor-actions">
        <button
          v-if="editing"
          id="delete-btn"
          type="button"
          class="danger"
          :disabled="saving"
          @click="askDelete"
        >
          移入垃圾桶</button
        ><span class="action-spacer"></span
        ><button
          id="cancel-edit"
          type="button"
          class="secondary"
          :disabled="saving || (!!editing && !dirty && !formError)"
          @click="closeForm"
        >
          {{ editing ? "撤销修改" : "取消新增" }}</button
        ><button
          id="save-entry"
          class="primary"
          type="submit"
          :disabled="saving || (!!editing && !dirty)"
        >
          {{ saving ? "保存中…" : editing ? "保存修改" : "保存条目" }}
        </button>
      </div>
    </form>
  </section>
  <section v-else class="detail empty">选择一个条目查看详情</section>
</template>
